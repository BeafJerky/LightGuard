use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use reqwest::Url;

pub const AUTO: &str = "auto";

const BLOCKED_HOST: &str = "更新地址不能指向本机或内网";

/// 判定一个 IPv4 字面量是否允许作为更新目标。
/// 这里只做字面量过滤：DNS rebinding 仍可能把公网域名解析到内网（见发布报告残留风险）。
fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    // 0.0.0.0/8（含未指定地址）、127.0.0.0/8 环回、10.0.0.0/8 私有。
    if a == 0 || a == 127 || a == 10 {
        return false;
    }
    // 172.16.0.0/12 私有。
    if a == 172 && (16..=31).contains(&b) {
        return false;
    }
    // 192.168.0.0/16 私有。
    if a == 192 && b == 168 {
        return false;
    }
    // 169.254.0.0/16 链路本地（含云厂商元数据地址 169.254.169.254）。
    if a == 169 && b == 254 {
        return false;
    }
    // 100.64.0.0/10 运营商级 NAT。
    if a == 100 && (64..=127).contains(&b) {
        return false;
    }
    // 198.18.0.0/15 基准测试保留地址（常被 VPN/沙箱劫持为内部服务）。
    if a == 198 && (18..=19).contains(&b) {
        return false;
    }
    // 224.0.0.0/4 组播、240.0.0.0/4 保留（含 255.255.255.255 广播）。
    if a >= 224 {
        return false;
    }
    if ip == Ipv4Addr::BROADCAST {
        return false;
    }
    true
}

/// 取 IPv6 地址前 96 位以内嵌 IPv4 的写法（IPv4 兼容/映射、NAT64）里内嵌的 IPv4。
fn embedded_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let segments = ip.segments();
    // IPv4 兼容地址 ::a.b.c.d（::/96）。`::` 与 `::1` 已在上层按未指定/环回拒绝，
    // `::ffff:a.b.c.d` 由 `to_ipv4_mapped` 优先处理，这里只兜住剩下的形式。
    if segments[..6].iter().all(|segment| *segment == 0) {
        let [a, b] = [segments[6], segments[7]];
        let v4 = Ipv4Addr::new(
            (a >> 8) as u8,
            (a & 0xff) as u8,
            (b >> 8) as u8,
            (b & 0xff) as u8,
        );
        return Some(v4);
    }
    // NAT64 64:ff9b::/96。
    if segments[0] == 0x0064
        && segments[1] == 0xff9b
        && segments[2..6].iter().all(|segment| *segment == 0)
    {
        let [a, b] = [segments[6], segments[7]];
        return Some(Ipv4Addr::new(
            (a >> 8) as u8,
            (a & 0xff) as u8,
            (b >> 8) as u8,
            (b & 0xff) as u8,
        ));
    }
    None
}

/// 判定一个 IPv6 字面量是否允许作为更新目标。
fn is_public_v6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    // IPv4 映射地址（::ffff:a.b.c.d）按内嵌的 IPv4 判定，避免绕过上面的过滤。
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_v4(mapped);
    }
    // IPv4 兼容地址与 NAT64 同样按内嵌 IPv4 判定，否则 `[64:ff9b::7f00:1]` 之类可直达内网。
    if let Some(embedded) = embedded_v4(ip) {
        return is_public_v4(embedded);
    }
    let segments = ip.segments();
    // fc00::/7 唯一本地地址。
    if segments[0] & 0xfe00 == 0xfc00 {
        return false;
    }
    // fe80::/10 单播链路本地。
    if segments[0] & 0xffc0 == 0xfe80 {
        return false;
    }
    true
}

fn is_public_domain(domain: &str) -> bool {
    // 大小写不敏感，且容忍 FQDN 末尾的点：`localhost.` 一样指向本机。
    let host = domain.trim_end_matches('.').to_ascii_lowercase();
    host != "localhost" && !host.ends_with(".localhost")
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => is_public_v6(ip),
    }
}

/// 更新源与更新资产都必须是公网地址：拒绝本机与内网字面量，
/// 避免被手改的更新源或清单把应用变成内网探测/下载器。
pub fn ensure_public_host(url: &Url) -> Result<(), String> {
    let host = url.host_str().ok_or_else(|| "更新地址缺少主机名".to_string())?;
    // `host_str` 给 IPv6 带上方括号（如 `[::1]`），剥掉后才是可解析的字面量。
    let allowed = if let Some(inner) = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        // 带方括号却解析不成 IP 的地址不可能是合法主机名（URL 已拒绝非 IPv6 的方括号形式）。
        inner.parse::<IpAddr>().is_ok_and(is_public_ip)
    } else if let Ok(ip) = host.parse::<IpAddr>() {
        is_public_ip(ip)
    } else {
        is_public_domain(host)
    };
    if allowed {
        Ok(())
    } else {
        Err(BLOCKED_HOST.to_string())
    }
}

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
    ensure_public_host(&url)?;
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

    fn assert_blocked(raw: &str) {
        let url = reqwest::Url::parse(raw).unwrap();
        let error = ensure_public_host(&url).unwrap_err();
        assert_eq!(error, "更新地址不能指向本机或内网", "unexpected error for {raw}");
        // 自定义更新源与资产走同一套判定：源本身也必须被拒绝。
        assert!(validate_source(raw).is_err(), "validate_source accepted {raw}");
    }

    #[test]
    fn rejects_loopback_private_and_reserved_hosts() {
        for raw in [
            "https://127.0.0.1/x",
            "https://10.0.0.1/x",
            "https://192.168.1.1/x",
            "https://169.254.1.1/x",
            "https://[::1]/x",
            "https://[fe80::1]/x",
            "https://localhost/x",
            "https://0.0.0.0/x",
            "https://100.64.1.1/x",
        ] {
            assert_blocked(raw);
        }
    }

    #[test]
    fn rejects_other_internal_ranges_and_hostname_forms() {
        for raw in [
            "https://172.16.0.1/x",
            "https://172.31.255.255/x",
            "https://198.18.0.1/x",
            "https://240.0.0.1/x",
            "https://0.1.2.3/x",
            "https://224.0.0.1/x",
            "https://255.255.255.255/x",
            "https://[fc00::1]/x",
            "https://[fd12:3456::1]/x",
            "https://[ff02::1]/x",
            "https://[::ffff:127.0.0.1]/x",
            "https://[::ffff:10.0.0.1]/x",
            "https://[64:ff9b::7f00:1]/x",
            "https://[64:ff9b::a00:1]/x",
            "https://[::127.0.0.1]/x",
            "https://[::10.0.0.1]/x",
            "https://LOCALHOST/x",
            "https://mirror.localhost/x",
            "https://localhost./x",
        ] {
            assert_blocked(raw);
        }
    }

    #[test]
    fn accepts_public_hosts_and_ip_literals() {
        for raw in [
            "https://github.com/x",
            "https://8.8.8.8/x",
            "https://[2606:4700:4700::1111]/x",
            "https://mirror.example.com/latest.json",
        ] {
            let url = reqwest::Url::parse(raw).unwrap();
            assert!(ensure_public_host(&url).is_ok(), "rejected public host {raw}");
        }
    }
}
