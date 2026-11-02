use reqwest::Url;

pub const AUTO: &str = "auto";

// 顺序即回退顺序；第一条是 GitHub 直连清单，后三条为镜像/缓存源。
pub const DEFAULT_SOURCES: [&str; 4] = [
    "https://raw.githubusercontent.com/BeafJerky/LightGuard/main/update/latest.json",
    "https://gh-proxy.com/https://raw.githubusercontent.com/BeafJerky/LightGuard/main/update/latest.cn.json",
    "https://ghproxy.net/https://raw.githubusercontent.com/BeafJerky/LightGuard/main/update/latest.cn.json",
    "https://cdn.jsdelivr.net/gh/BeafJerky/LightGuard@main/update/latest.cn.json",
];

pub fn resolve_sources(selected: &str) -> Result<Vec<Url>, String> {
    let mut sources = Vec::new();
    if selected != AUTO {
        sources.push(validate_source(selected)?);
    }
    for raw in DEFAULT_SOURCES {
        let url = Url::parse(raw).map_err(|error| format!("内置更新源无效：{error}"))?;
        if !sources.contains(&url) {
            sources.push(url);
        }
    }
    Ok(sources)
}

pub fn validate_source(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|_| "更新源不是有效 URL".to_string())?;
    if url.scheme() != "https" {
        return Err("更新源必须使用 HTTPS".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("更新源不能包含用户名或密码".to_string());
    }
    if url.host_str().is_none() {
        return Err("更新源缺少主机名".to_string());
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_keeps_official_sources() {
        let sources = resolve_sources(AUTO).unwrap();
        assert!(sources[0].host_str().unwrap().contains("raw.githubusercontent.com"));
        assert!(sources.len() >= 4);
    }

    #[test]
    fn custom_source_goes_first_and_is_deduplicated() {
        let sources = resolve_sources("https://mirror.example.com/latest.json").unwrap();
        assert_eq!(sources[0].host_str(), Some("mirror.example.com"));
        assert_eq!(sources.iter().filter(|url| url.host_str() == Some("mirror.example.com")).count(), 1);
    }

    #[test]
    fn rejects_non_https_and_credentials() {
        assert!(validate_source("http://example.com/latest.json").is_err());
        assert!(validate_source("https://user:pass@example.com/latest.json").is_err());
        assert!(validate_source("not a url").is_err());
    }
}
