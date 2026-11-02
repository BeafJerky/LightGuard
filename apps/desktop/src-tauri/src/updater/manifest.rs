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

/// 清单可能来自直连或镜像；镜像清单里的 url 已带一层前缀（如 latest.cn.json）。
/// 先还原成上游 GitHub 地址，再统一生成候选，避免拼出双重前缀。
fn upstream_asset(direct: &Url) -> Url {
    let text = direct.as_str();
    for mirror in MIRRORS {
        if let Some(rest) = text.strip_prefix(mirror) {
            if let Ok(url) = Url::parse(rest) {
                return url;
            }
        }
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
        assert!(candidates.len() >= 3);
        assert!(candidates.iter().all(|url| url.scheme() == "https"));
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
}
