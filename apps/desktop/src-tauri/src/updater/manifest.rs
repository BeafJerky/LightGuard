use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;

pub const TARGET: &str = "windows-x86_64";
const MIRRORS: [&str; 4] = [
    "https://gh-proxy.com/",
    "https://ghproxy.net/",
    "https://ghfast.top/",
    "https://gh-proxy.org/",
];

#[derive(Debug, Clone, Deserialize)]
pub struct RawPlatform {
    pub url: String,
    pub signature: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawRelease {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub pub_date: Option<String>,
    pub platforms: std::collections::BTreeMap<String, RawPlatform>,
}

#[derive(Debug, Clone)]
pub struct ParsedRelease {
    pub version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub platform: RawPlatform,
    pub asset_url: Url,
}

pub fn parse_release(raw: &Value) -> Result<ParsedRelease, String> {
    let release: RawRelease =
        serde_json::from_value(raw.clone()).map_err(|error| format!("更新清单格式无效：{error}"))?;
    let platform = release
        .platforms
        .get(TARGET)
        .cloned()
        .ok_or_else(|| format!("更新清单缺少 {TARGET} 平台"))?;
    let asset_url = Url::parse(&platform.url).map_err(|error| format!("更新资产地址无效：{error}"))?;
    if asset_url.scheme() != "https" {
        return Err("更新资产地址必须使用 HTTPS".to_string());
    }
    if !asset_url.username().is_empty() || asset_url.password().is_some() {
        return Err("更新资产地址不能包含凭据".to_string());
    }
    // 清单可能来自镜像，URL 完全由远端控制：拒绝内网地址，避免更新变成内网下载器。
    crate::updater::source::ensure_public_host(&asset_url)?;
    Ok(ParsedRelease {
        version: release.version,
        notes: release.notes,
        pub_date: release.pub_date,
        platform,
        asset_url,
    })
}

pub fn is_newer(current: &str, remote: &str) -> bool {
    let Ok(current) = semver::Version::parse(current) else { return false; };
    let Ok(remote) = semver::Version::parse(remote) else { return false; };
    remote > current
}

/// 只有上游是 GitHub 时才生成 gh-proxy 候选：企业内网的私有更新源不能被拼进公共代理，
/// 否则等于把内网地址抄送给第三方。
fn is_github_host(url: &Url) -> bool {
    let Some(host) = url.host_str() else { return false; };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "github.com" || host.ends_with(".githubusercontent.com")
}

/// 清单可能来自直连或镜像；镜像清单里的 url 已带一层前缀（如 latest.cn.json）。
/// 先还原成上游 GitHub 地址，再统一生成候选，避免拼出双重前缀。
///
/// 剥离前缀后剩下的部分是远端完全可控的字符串，必须按新 URL 重新校验：
/// 否则 `https://gh-proxy.com/https://127.0.0.1:9/x.exe` 会以外层公网主机通过校验，
/// 再在下载阶段回退到内网裸地址。
fn upstream_asset(direct: &Url) -> Url {
    let text = direct.as_str();
    for mirror in MIRRORS {
        let Some(rest) = text.strip_prefix(mirror) else { continue; };
        let Ok(upstream) = Url::parse(rest) else { continue; };
        if upstream.scheme() != "https" {
            continue;
        }
        if crate::updater::source::ensure_public_host(&upstream).is_err() {
            continue;
        }
        if !is_github_host(&upstream) {
            continue;
        }
        return upstream;
    }
    direct.clone()
}

fn push_unique(list: &mut Vec<Url>, url: Url) {
    if !list.contains(&url) {
        list.push(url);
    }
}

pub fn asset_candidates(direct: &Url) -> Vec<Url> {
    let upstream = upstream_asset(direct);
    let mut candidates = vec![direct.clone()];
    push_unique(&mut candidates, upstream.clone());
    if !is_github_host(&upstream) {
        return candidates;
    }
    for mirror in MIRRORS {
        if let Ok(url) = Url::parse(&format!("{mirror}{upstream}")) {
            push_unique(&mut candidates, url);
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest(version: &str, url: &str) -> serde_json::Value {
        json!({
            "version": version,
            "notes": "notes",
            "pub_date": "2026-11-02T00:00:00Z",
            "platforms": {
                "windows-x86_64": {
                    "url": url,
                    "signature": "sig",
                    "size": 123
                }
            }
        })
    }

    #[test]
    fn parses_windows_platform() {
        let parsed = parse_release(&manifest("0.15.0", "https://example.com/a.exe")).unwrap();
        assert_eq!(parsed.version, "0.15.0");
        assert_eq!(parsed.platform.size, Some(123));
    }

    #[test]
    fn rejects_missing_platform() {
        let raw = json!({ "version": "0.15.0", "platforms": {} });
        assert!(parse_release(&raw).is_err());
    }

    #[test]
    fn rejects_plain_http_assets() {
        assert!(parse_release(&manifest("0.15.0", "http://example.com/a.exe")).is_err());
    }

    #[test]
    fn rejects_asset_urls_with_credentials() {
        assert!(parse_release(&manifest("0.15.0", "https://user:pass@example.com/a.exe")).is_err());
    }

    #[test]
    fn rejects_asset_urls_on_private_hosts() {
        // 清单由远端控制：资产地址指向内网时必须整体拒绝，而不是留给下载阶段。
        for url in [
            "https://127.0.0.1/a.exe",
            "https://10.0.0.1/a.exe",
            "https://192.168.1.1/a.exe",
            "https://[::1]/a.exe",
            "https://localhost/a.exe",
        ] {
            let error = parse_release(&manifest("0.15.0", url)).unwrap_err();
            assert_eq!(error, "更新地址不能指向本机或内网", "unexpected error for {url}");
        }
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.14.1", "0.15.0"));
        assert!(!is_newer("0.15.0", "0.15.0"));
        assert!(!is_newer("0.15.0", "0.14.1"));
    }

    #[test]
    fn builds_direct_then_mirror_candidates() {
        let direct = reqwest::Url::parse("https://github.com/a/b.exe").unwrap();
        let candidates = asset_candidates(&direct);
        assert_eq!(candidates[0], direct);
        assert_eq!(candidates.len(), 5);
        assert!(candidates.iter().all(|url| url.scheme() == "https"));
    }

    #[test]
    fn non_github_assets_get_no_mirrors() {
        // 非 GitHub 上游（例如企业私有更新源）不能被拼进公共 gh-proxy。
        let direct = reqwest::Url::parse("https://updates.corp.example/a.exe").unwrap();
        let candidates = asset_candidates(&direct);
        assert_eq!(candidates, vec![direct]);
    }

    #[test]
    fn strips_an_existing_mirror_prefix() {
        let mirrored =
            reqwest::Url::parse("https://gh-proxy.com/https://github.com/a/b.exe").unwrap();
        let candidates = asset_candidates(&mirrored);
        assert!(candidates
            .iter()
            .any(|url| url.as_str() == "https://github.com/a/b.exe"));
        assert!(!candidates
            .iter()
            .any(|url| url.as_str().contains("gh-proxy.com/https://gh-proxy.com")));
    }

    fn assert_not_restripped(raw: &str) {
        let direct = reqwest::Url::parse(raw).unwrap();
        // 外层是公网镜像主机，parse_release 会放行；剥离前缀后必须重新校验，不能多出候选。
        assert!(parse_release(&manifest("0.15.0", raw)).is_ok());
        let candidates = asset_candidates(&direct);
        assert_eq!(candidates.len(), 1, "unexpected extra candidates for {raw}: {candidates:?}");
        assert_eq!(candidates[0], direct);
        assert!(candidates
            .iter()
            .all(|url| crate::updater::source::ensure_public_host(url).is_ok()));
    }

    #[test]
    fn mirror_prefixed_loopback_assets_are_not_stripped() {
        // 回归：曾把 `https://gh-proxy.com/https://127.0.0.1:9/x.exe` 剥成环回地址，
        // 外层主机是公网镜像所以 parse_release 放行，下载阶段再回退到内网。
        assert_not_restripped("https://gh-proxy.com/https://127.0.0.1:9/x.exe");
    }

    #[test]
    fn mirror_prefixed_private_assets_are_not_stripped() {
        assert_not_restripped("https://gh-proxy.com/https://10.0.0.1/x.exe");
    }

    #[test]
    fn mirror_prefixed_non_github_assets_are_not_stripped() {
        assert_not_restripped("https://gh-proxy.com/https://example.com/a.exe");
    }
}
