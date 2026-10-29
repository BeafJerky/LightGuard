//! `daemon.json` 的纯函数协议：只合并 `registry-mirrors` 与 `proxies`，
//! 其余键原样保留；预览只展示脱敏后的值。

use serde::{Deserialize, Serialize};

use crate::docker::validate::{no_proxy, proxy_url, redact_url};
use crate::ssh::error::SshError;

pub const DAEMON_PATH: &str = "/etc/docker/daemon.json";
pub const MAX_MIRRORS: usize = 10;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonPayload {
    #[serde(default)]
    pub mirrors: Option<Vec<String>>,
    #[serde(default)]
    pub http_proxy: Option<String>,
    #[serde(default)]
    pub https_proxy: Option<String>,
    #[serde(default)]
    pub no_proxy: Option<String>,
    /// 显式清除的代理键：`httpProxy` / `httpsProxy` / `noProxy`。
    #[serde(default)]
    pub clear: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonMerge {
    pub rendered: String,
    pub changed: Vec<String>,
    pub proxy_changed: bool,
}

/// 只改写 `registry-mirrors` 与 `proxies`；其余顶层键的值原样保留。
pub fn merge(existing: &str, payload: &DaemonPayload) -> Result<DaemonMerge, SshError> {
    let trimmed = existing.trim();
    let source = if trimmed.is_empty() { "{}" } else { trimmed };
    let mut value: serde_json::Value = serde_json::from_str(source)
        .map_err(|error| SshError::daemon_invalid(format!("daemon.json 解析失败：{error}")))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| SshError::daemon_invalid("daemon.json 顶层必须是对象"))?;

    if let Some(key) = object.get("registry-mirrors") {
        if !key
            .as_array()
            .map(|rows| rows.iter().all(serde_json::Value::is_string))
            .unwrap_or(false)
        {
            return Err(SshError::daemon_invalid(
                "registry-mirrors 必须是字符串数组",
            ));
        }
    }
    if let Some(key) = object.get("proxies") {
        if !key.is_object() {
            return Err(SshError::daemon_invalid("proxies 必须是对象"));
        }
    }

    let mut changed = Vec::new();
    let mut proxy_changed = false;

    if let Some(mirrors) = &payload.mirrors {
        let mut cleaned: Vec<String> = Vec::new();
        for mirror in mirrors {
            let mirror = mirror.trim();
            if mirror.is_empty() {
                continue;
            }
            let mirror = proxy_url(mirror)?;
            if !cleaned.contains(&mirror) {
                cleaned.push(mirror);
            }
        }
        cleaned.truncate(MAX_MIRRORS);
        let next =
            serde_json::Value::Array(cleaned.into_iter().map(serde_json::Value::String).collect());
        // 前端只要能读到配置就会带上 mirrors（可能是空数组）：原文件没有该键时
        // 不要顺带写入 `[]`，否则代理保存会多出一个无关键，且“配置没有变化”失效。
        let had_key = object.contains_key("registry-mirrors");
        let has_values = next.as_array().is_some_and(|rows| !rows.is_empty());
        if (had_key || has_values) && object.get("registry-mirrors") != Some(&next) {
            object.insert("registry-mirrors".to_string(), next);
            changed.push("registry-mirrors".to_string());
        }
    }

    let mut proxies = object
        .remove("proxies")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    let apply = |proxies: &mut serde_json::Map<String, serde_json::Value>,
                 key: &str,
                 value: &Option<String>,
                 clear: bool,
                 changed: &mut Vec<String>,
                 proxy_changed: &mut bool|
     -> Result<(), SshError> {
        if clear {
            if proxies.remove(key).is_some() {
                changed.push(format!("proxies.{key}"));
                *proxy_changed = true;
            }
            return Ok(());
        }
        let Some(raw) = value else { return Ok(()) };
        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(());
        }
        let next = if key == "no-proxy" {
            no_proxy(raw)?
        } else {
            proxy_url(raw)?
        };
        let next = serde_json::Value::String(next);
        if proxies.get(key) != Some(&next) {
            proxies.insert(key.to_string(), next);
            changed.push(format!("proxies.{key}"));
            *proxy_changed = true;
        }
        Ok(())
    };

    apply(
        &mut proxies,
        "http-proxy",
        &payload.http_proxy,
        payload.clear.iter().any(|key| key == "httpProxy"),
        &mut changed,
        &mut proxy_changed,
    )?;
    apply(
        &mut proxies,
        "https-proxy",
        &payload.https_proxy,
        payload.clear.iter().any(|key| key == "httpsProxy"),
        &mut changed,
        &mut proxy_changed,
    )?;
    apply(
        &mut proxies,
        "no-proxy",
        &payload.no_proxy,
        payload.clear.iter().any(|key| key == "noProxy"),
        &mut changed,
        &mut proxy_changed,
    )?;

    // 清空代理同样需要重启 daemon，因此 `changed` / `proxy_changed` 保留原记录；
    // 空对象不再写回，避免留下无意义的 `"proxies": {}`。
    if !proxies.is_empty() {
        object.insert("proxies".to_string(), serde_json::Value::Object(proxies));
    }

    let mut rendered = serde_json::to_string_pretty(&value)
        .map_err(|error| SshError::internal(error.to_string()))?;
    rendered.push('\n');
    Ok(DaemonMerge {
        rendered,
        changed,
        proxy_changed,
    })
}

/// 预览只展示键名与脱敏值，绝不返回凭据。
pub fn preview_lines(merged: &DaemonMerge) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&merged.rendered) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if let Some(mirrors) = value
        .get("registry-mirrors")
        .and_then(|value| value.as_array())
    {
        for mirror in mirrors {
            if let Some(mirror) = mirror.as_str() {
                lines.push(format!("加速源：{}", redact_url(mirror)));
            }
        }
    }
    if let Some(proxies) = value.get("proxies").and_then(|value| value.as_object()) {
        for (key, proxy) in proxies {
            if let Some(proxy) = proxy.as_str() {
                lines.push(format!("{key}：{}", redact_url(proxy)));
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(json: serde_json::Value) -> DaemonPayload {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn replaces_mirrors_and_preserves_other_keys() {
        let existing = r#"{"log-driver":"json-file","registry-mirrors":["https://old"]}"#;
        let merged = merge(
            existing,
            &payload(serde_json::json!({ "mirrors": ["https://mirror.example.com"] })),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&merged.rendered).unwrap();
        assert_eq!(value["log-driver"], "json-file");
        assert_eq!(value["registry-mirrors"][0], "https://mirror.example.com");
        assert!(merged.changed.contains(&"registry-mirrors".to_string()));
        assert!(!merged.proxy_changed);
    }

    #[test]
    fn blank_proxy_fields_keep_existing_values_and_clear_removes_them() {
        let existing =
            r#"{"proxies":{"http-proxy":"http://old:3128","https-proxy":"http://old:3129"}}"#;
        let keep = merge(
            existing,
            &payload(serde_json::json!({ "httpProxy": "", "httpsProxy": "" })),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&keep.rendered).unwrap();
        assert_eq!(value["proxies"]["http-proxy"], "http://old:3128");
        assert_eq!(value["proxies"]["https-proxy"], "http://old:3129");
        assert!(keep.changed.is_empty());

        let cleared = merge(
            existing,
            &payload(serde_json::json!({ "clear": ["httpProxy", "httpsProxy"] })),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&cleared.rendered).unwrap();
        assert!(
            value.get("proxies").is_none(),
            "empty proxies object is removed"
        );
        assert!(cleared.proxy_changed);
    }

    #[test]
    fn sets_http_and_https_proxies() {
        let merged = merge(
            "{}",
            &payload(serde_json::json!({
                "httpProxy": "http://u:p@proxy:3128",
                "noProxy": "localhost,10.0.0.0/8"
            })),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&merged.rendered).unwrap();
        assert_eq!(value["proxies"]["http-proxy"], "http://u:p@proxy:3128");
        assert_eq!(value["proxies"]["no-proxy"], "localhost,10.0.0.0/8");
        assert!(merged.proxy_changed);
        let preview = preview_lines(&merged).join("\n");
        assert!(!preview.contains("u:p@"), "credentials must be masked");
        assert!(preview.contains("http://***@proxy:3128"));
    }

    #[test]
    fn rejects_invalid_existing_structure_and_bad_urls() {
        assert_eq!(
            merge("[1,2]", &payload(serde_json::json!({})))
                .unwrap_err()
                .code,
            "DAEMON_INVALID"
        );
        assert_eq!(
            merge(r#"{"proxies":"nope"}"#, &payload(serde_json::json!({})))
                .unwrap_err()
                .code,
            "DAEMON_INVALID"
        );
        assert_eq!(
            merge(
                "{}",
                &payload(serde_json::json!({ "httpProxy": "socks5://x" }))
            )
            .unwrap_err()
            .code,
            "DAEMON_INVALID"
        );
    }

    #[test]
    fn mirrors_are_deduped_and_capped() {
        let mirrors: Vec<String> = (0..12)
            .map(|index| format!("https://m{index}.example.com"))
            .collect();
        let merged = merge("{}", &payload(serde_json::json!({ "mirrors": mirrors }))).unwrap();
        let value: serde_json::Value = serde_json::from_str(&merged.rendered).unwrap();
        assert_eq!(value["registry-mirrors"].as_array().unwrap().len(), 10);
    }

    #[test]
    fn empty_mirrors_do_not_create_a_key_for_proxy_only_saves() {
        let merged = merge(
            "{}",
            &payload(serde_json::json!({
                "mirrors": [],
                "httpProxy": "http://proxy:3128"
            })),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&merged.rendered).unwrap();
        assert!(
            value.get("registry-mirrors").is_none(),
            "absent mirror key stays absent when the payload is empty"
        );
        assert_eq!(
            merged.changed,
            vec!["proxies.http-proxy".to_string()],
            "only the proxy key counts as changed"
        );

        let noop = merge(
            r#"{"proxies":{"http-proxy":"http://proxy:3128"}}"#,
            &payload(serde_json::json!({
                "mirrors": [],
                "httpProxy": "http://proxy:3128"
            })),
        )
        .unwrap();
        assert!(noop.changed.is_empty(), "a genuine no-op stays a no-op");

        let cleared = merge(
            r#"{"registry-mirrors":["https://old"]}"#,
            &payload(serde_json::json!({ "mirrors": [] })),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&cleared.rendered).unwrap();
        assert_eq!(value["registry-mirrors"], serde_json::json!([]));
        assert!(cleared.changed.contains(&"registry-mirrors".to_string()));
    }
}
