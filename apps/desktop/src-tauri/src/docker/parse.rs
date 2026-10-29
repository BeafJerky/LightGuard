//! Docker CLI 输出解析。所有函数对空行、截断与非法 JSON 容错：能解析的尽量保留，
//! 解析不了的整行跳过，绝不因为一行坏数据让整页报错。

use serde_json::Value;

use crate::docker::dto::{DockerContainer, DockerStatsSample};

pub fn containers(stdout: &str) -> Vec<DockerContainer> {
    stdout.lines().filter_map(parse_container).collect()
}

fn parse_container(line: &str) -> Option<DockerContainer> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    let id = text(&value, "ID");
    if id.is_empty() {
        return None;
    }
    let short_id: String = id.chars().take(12).collect();
    let labels = text(&value, "Labels");
    Some(DockerContainer {
        id,
        short_id,
        name: text(&value, "Names").trim_start_matches('/').to_string(),
        image: text(&value, "Image"),
        command: text(&value, "Command").chars().take(160).collect(),
        state: text(&value, "State"),
        status: text(&value, "Status"),
        created_at: text(&value, "CreatedAt"),
        project: label(&labels, "com.docker.compose.project"),
        service: label(&labels, "com.docker.compose.service"),
        ports: text(&value, "Ports"),
        ..Default::default()
    })
}

pub fn stats(stdout: &str) -> Vec<DockerStatsSample> {
    stdout
        .lines()
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line.trim()).ok()?;
            let id = value.get("ID").and_then(Value::as_str)?.to_string();
            let (mem_usage, mem_limit) = split_pair(&text(&value, "MemUsage"));
            let (net_rx, net_tx) = split_pair(&text(&value, "NetIO"));
            Some(DockerStatsSample {
                id,
                cpu_percent: percent(&text(&value, "CPUPerc")),
                mem_usage,
                mem_limit,
                mem_percent: percent(&text(&value, "MemPerc")),
                net_rx,
                net_tx,
            })
        })
        .collect()
}

pub fn merge_stats(containers: &mut [DockerContainer], stats: &[DockerStatsSample]) {
    for container in containers {
        let Some(sample) = stats.iter().find(|sample| sample.id == container.id) else {
            continue;
        };
        container.cpu_percent = Some(sample.cpu_percent);
        container.mem_percent = Some(sample.mem_percent);
        container.mem_usage = sample.mem_usage.clone();
        container.mem_limit = sample.mem_limit.clone();
        container.net_rx = sample.net_rx.clone();
        container.net_tx = sample.net_tx.clone();
    }
}

pub(crate) fn text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn label(labels: &str, key: &str) -> String {
    labels
        .split(',')
        .find_map(|pair| {
            pair.split_once('=')
                .filter(|(name, _)| *name == key)
                .map(|(_, value)| value.to_string())
        })
        .unwrap_or_default()
}

fn percent(value: &str) -> f64 {
    value
        .trim()
        .trim_end_matches('%')
        .parse::<f64>()
        .unwrap_or(0.0)
}

fn split_pair(value: &str) -> (String, String) {
    match value.split_once(" / ") {
        Some((left, right)) => (left.trim().to_string(), right.trim().to_string()),
        None => (value.trim().to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = r#"{"Command":"\"/docker-entrypoint.sh nginx -g 'daemon off;'\"","CreatedAt":"2026-10-01 10:00:00 +0800 CST","ID":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","Image":"nginx:1.27","Labels":"com.docker.compose.project=shop,com.docker.compose.service=web,other=1","Names":"shop-web-1","Ports":"0.0.0.0:8080->80/tcp, :::8080->80/tcp","State":"running","Status":"Up 2 hours","Networks":"bridge"}
{"Command":"sleep infinity","CreatedAt":"2026-09-01 08:00:00 +0800 CST","ID":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","Image":"alpine","Labels":"","Names":"idle","Ports":"","State":"exited","Status":"Exited (0) 1 day ago"}
"#;

    const STATS: &str = r#"{"BlockIO":"0B / 0B","CPUPerc":"0.15%","Container":"aaaa","ID":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","MemPerc":"0.42%","MemUsage":"12.3MiB / 1.9GiB","Name":"shop-web-1","NetIO":"1.2kB / 3.4kB","PIDs":"5"}
"#;

    #[test]
    fn parses_containers_and_compose_labels() {
        let rows = containers(PS);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].short_id, "aaaaaaaaaaaa");
        assert_eq!(rows[0].name, "shop-web-1");
        assert_eq!(rows[0].project, "shop");
        assert_eq!(rows[0].service, "web");
        assert_eq!(rows[0].state, "running");
        assert!(rows[0].ports.contains("8080"));
        assert_eq!(rows[1].project, "");
        assert_eq!(rows[1].state, "exited");
    }

    #[test]
    fn tolerates_blank_and_broken_lines() {
        let mixed = format!("\nnot-json\n{PS}");
        assert_eq!(containers(&mixed).len(), 2);
    }

    #[test]
    fn merges_stats_by_full_id() {
        let mut rows = containers(PS);
        merge_stats(&mut rows, &stats(STATS));
        assert_eq!(rows[0].cpu_percent, Some(0.15));
        assert_eq!(rows[0].mem_percent, Some(0.42));
        assert_eq!(rows[0].mem_usage, "12.3MiB");
        assert_eq!(rows[0].mem_limit, "1.9GiB");
        assert_eq!(rows[0].net_rx, "1.2kB");
        assert_eq!(rows[0].net_tx, "3.4kB");
        assert_eq!(rows[1].cpu_percent, None, "no stats for exited container");
    }

    #[test]
    fn stats_ignore_rows_without_id() {
        assert!(stats("{\"CPUPerc\":\"1%\"}\n").is_empty());
    }
}
