//! Docker CLI 输出解析。所有函数对空行、截断与非法 JSON 容错：能解析的尽量保留，
//! 解析不了的整行跳过，绝不因为一行坏数据让整页报错。

use serde_json::Value;

use crate::docker::dto::{
    DockerContainer, DockerContainerDetail, DockerCounts, DockerDf, DockerEnvVar, DockerImage,
    DockerInfo, DockerMount, DockerNetwork, DockerPort, DockerStatsSample, RegistrySearchItem,
};
use crate::docker::validate::mask_env_value;
use crate::ssh::error::SshError;

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

const MAX_ENV_ROWS: usize = 200;

pub fn container_detail(json: &str) -> Result<DockerContainerDetail, SshError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| SshError::docker_failed(format!("容器详情解析失败：{error}")))?;
    let item = value
        .as_array()
        .and_then(|rows| rows.first())
        .unwrap_or(&value);
    if item.is_null() || !item.is_object() {
        return Err(SshError::docker_failed("容器不存在或已删除"));
    }

    let command = item
        .pointer("/Config/Cmd")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();

    let mut env = Vec::new();
    let mut env_truncated = false;
    if let Some(rows) = item.pointer("/Config/Env").and_then(Value::as_array) {
        for row in rows {
            let Some(pair) = row.as_str() else { continue };
            if env.len() >= MAX_ENV_ROWS {
                env_truncated = true;
                break;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            env.push(DockerEnvVar {
                key: key.to_string(),
                value: mask_env_value(key, value),
            });
        }
    }

    let mut mounts = Vec::new();
    if let Some(rows) = item.get("Mounts").and_then(Value::as_array) {
        for row in rows {
            mounts.push(DockerMount {
                source: pointer_text(row, "/Source"),
                destination: pointer_text(row, "/Destination"),
                mode: pointer_text(row, "/Mode"),
                read_only: !row.get("RW").and_then(Value::as_bool).unwrap_or(true),
            });
        }
    }

    let mut networks = Vec::new();
    if let Some(map) = item
        .pointer("/NetworkSettings/Networks")
        .and_then(Value::as_object)
    {
        for (name, row) in map {
            networks.push(DockerNetwork {
                name: name.clone(),
                ip: pointer_text(row, "/IPAddress"),
                gateway: pointer_text(row, "/Gateway"),
                mac: pointer_text(row, "/MacAddress"),
            });
        }
    }

    let mut ports = Vec::new();
    if let Some(map) = item
        .pointer("/NetworkSettings/Ports")
        .and_then(Value::as_object)
    {
        for (key, bindings) in map {
            let (container_port, protocol) = key.split_once('/').unwrap_or((key.as_str(), ""));
            let mut bound = false;
            if let Some(rows) = bindings.as_array() {
                for row in rows {
                    let host_ip = pointer_text(row, "/HostIp");
                    let host_port = pointer_text(row, "/HostPort");
                    if host_ip.is_empty() && host_port.is_empty() {
                        continue;
                    }
                    bound = true;
                    ports.push(DockerPort {
                        host_ip,
                        host_port,
                        container_port: container_port.to_string(),
                        protocol: protocol.to_string(),
                    });
                }
            }
            if !bound {
                ports.push(DockerPort {
                    host_ip: String::new(),
                    host_port: String::new(),
                    container_port: container_port.to_string(),
                    protocol: protocol.to_string(),
                });
            }
        }
    }
    ports.sort_by(|left, right| left.container_port.cmp(&right.container_port));

    Ok(DockerContainerDetail {
        id: pointer_text(item, "/Id"),
        name: pointer_text(item, "/Name").trim_start_matches('/').to_string(),
        image: pointer_text(item, "/Config/Image"),
        command,
        state: pointer_text(item, "/State/Status"),
        status: pointer_text(item, "/State/Status"),
        health: pointer_text(item, "/State/Health/Status"),
        restart_policy: pointer_text(item, "/HostConfig/RestartPolicy/Name"),
        created_at: pointer_text(item, "/Created"),
        started_at: pointer_text(item, "/State/StartedAt"),
        ports,
        mounts,
        networks,
        env,
        env_truncated,
    })
}

fn pointer_text(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub fn images(stdout: &str) -> Vec<DockerImage> {
    let mut rows: Vec<DockerImage> = Vec::new();
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        let raw_id = text(&value, "ID");
        if raw_id.is_empty() {
            continue;
        }
        let id = raw_id.trim_start_matches("sha256:").to_string();
        let repo = text(&value, "Repository");
        let tag = text(&value, "Tag");
        let containers = text(&value, "Containers").parse::<i64>().unwrap_or(0);
        if let Some(image) = rows.iter_mut().find(|image| image.id == id) {
            if repo != "<none>" && tag != "<none>" && !image.tags.contains(&tag) {
                if image.repo.is_empty() || image.repo == "<none>" {
                    image.repo = repo;
                }
                image.tags.push(tag);
            }
            image.containers = image.containers.max(containers);
            continue;
        }
        let short_id: String = id.chars().take(12).collect();
        rows.push(DockerImage {
            short_id,
            id,
            repo,
            tags: if tag == "<none>" { Vec::new() } else { vec![tag] },
            size: text(&value, "Size"),
            created_at: text(&value, "CreatedAt"),
            containers,
            dangling: false,
        });
    }
    for image in &mut rows {
        image.dangling = image.tags.is_empty() || image.repo == "<none>";
        if image.repo == "<none>" {
            image.repo = String::new();
        }
    }
    rows
}

pub fn overview_parts(stdout: &str) -> (DockerInfo, DockerCounts) {
    let value: Value = serde_json::from_str(stdout.trim()).unwrap_or(Value::Null);
    let mirrors = value
        .pointer("/RegistryConfig/Mirrors")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let number = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    (
        DockerInfo {
            version: pointer_text(&value, "/ServerVersion"),
            api_version: pointer_text(&value, "/APIVersion"),
            storage_driver: pointer_text(&value, "/Driver"),
            data_root: pointer_text(&value, "/DockerRootDir"),
            mirrors,
        },
        DockerCounts {
            containers_running: number("/ContainersRunning"),
            containers_total: number("/Containers"),
            images: number("/Images"),
        },
    )
}

pub fn df(stdout: &str) -> Option<DockerDf> {
    let mut usage = DockerDf {
        images_size: "—".to_string(),
        containers_size: "—".to_string(),
        volumes_size: "—".to_string(),
        build_cache_size: "—".to_string(),
        reclaimable: "—".to_string(),
    };
    let mut found = false;
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        let kind = text(&value, "Type");
        let size = text(&value, "Size");
        match kind.as_str() {
            "Images" => {
                usage.images_size = size;
                usage.reclaimable = text(&value, "Reclaimable");
                found = true;
            }
            "Containers" => usage.containers_size = size,
            "Local Volumes" => usage.volumes_size = size,
            "Build Cache" => usage.build_cache_size = size,
            _ => {}
        }
    }
    found.then_some(usage)
}

pub fn search(stdout: &str) -> Vec<RegistrySearchItem> {
    stdout
        .lines()
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line.trim()).ok()?;
            let name = text(&value, "Name");
            if name.is_empty() {
                return None;
            }
            Some(RegistrySearchItem {
                name,
                description: text(&value, "Description"),
                stars: text(&value, "StarCount").parse::<i64>().unwrap_or(0),
                official: boolish(value.get("IsOfficial")),
                automated: boolish(value.get("IsAutomated")),
            })
        })
        .collect()
}

fn boolish(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::String(text)) => text.eq_ignore_ascii_case("true") || text == "[OK]",
        _ => false,
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

    const INSPECT: &str = r#"[{
      "Id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "Created":"2026-10-01T02:00:00.000000000Z",
      "Name":"/shop-web-1",
      "Config":{
        "Image":"nginx:1.27",
        "Cmd":["nginx","-g","daemon off;"],
        "Env":["PATH=/usr/bin","DB_PASSWORD=hunter2","API_TOKEN=abc123","PORT=80"]
      },
      "State":{"Status":"running","StartedAt":"2026-10-01T02:00:05.000000000Z","Health":{"Status":"healthy"}},
      "HostConfig":{"RestartPolicy":{"Name":"always"}},
      "Mounts":[{"Source":"/data/html","Destination":"/usr/share/nginx/html","Mode":"rw","RW":true}],
      "NetworkSettings":{
        "Ports":{"80/tcp":[{"HostIp":"0.0.0.0","HostPort":"8080"}],"443/tcp":null},
        "Networks":{"bridge":{"IPAddress":"172.17.0.2","Gateway":"172.17.0.1","MacAddress":"02:42:ac:11:00:02"}}
      }
    }]"#;

    #[test]
    fn parses_detail_and_masks_sensitive_env() {
        let detail = container_detail(INSPECT).unwrap();
        assert_eq!(detail.id, "a".repeat(64));
        assert_eq!(detail.name, "shop-web-1");
        assert_eq!(detail.image, "nginx:1.27");
        assert_eq!(detail.command, "nginx -g daemon off;");
        assert_eq!(detail.state, "running");
        assert_eq!(detail.health, "healthy");
        assert_eq!(detail.restart_policy, "always");
        assert_eq!(detail.mounts.len(), 1);
        assert_eq!(detail.mounts[0].destination, "/usr/share/nginx/html");
        assert!(detail.mounts[0].read_only == false);
        assert_eq!(detail.ports.len(), 2);
        assert_eq!(detail.ports[0].container_port, "443");
        assert_eq!(detail.ports[0].host_port, "", "unmapped port has no host");
        assert_eq!(detail.ports[1].host_port, "8080");
        assert_eq!(detail.networks[0].ip, "172.17.0.2");
        let password = detail.env.iter().find(|row| row.key == "DB_PASSWORD").unwrap();
        assert_eq!(password.value, "••••");
        let token = detail.env.iter().find(|row| row.key == "API_TOKEN").unwrap();
        assert_eq!(token.value, "••••");
        let port = detail.env.iter().find(|row| row.key == "PORT").unwrap();
        assert_eq!(port.value, "80");
        let json = serde_json::to_string(&detail).unwrap();
        assert!(!json.contains("hunter2"));
        assert!(!json.contains("abc123"));
    }

    #[test]
    fn rejects_invalid_inspect_json_and_missing_container() {
        assert_eq!(
            container_detail("not-json").unwrap_err().code,
            "DOCKER_FAILED"
        );
        assert_eq!(
            container_detail("null").unwrap_err().code,
            "DOCKER_FAILED"
        );
    }

    #[test]
    fn truncates_env_at_the_row_cap() {
        let env: Vec<String> = (0..=200)
            .map(|index| format!("KEY_{index}=value_{index}"))
            .collect();
        let inspect = serde_json::json!({
            "Id": "a".repeat(64),
            "Config": { "Env": env },
        })
        .to_string();
        let detail = container_detail(&inspect).unwrap();
        assert_eq!(detail.env.len(), 200);
        assert!(detail.env_truncated);
        let last = detail.env.last().unwrap();
        assert_eq!(last.key, "KEY_199");
        assert!(
            detail.env.iter().all(|row| row.key != "KEY_200"),
            "the 201st row is dropped"
        );
    }

    #[test]
    fn merges_tags_by_image_id() {
        let raw = r#"{"Containers":"N/A","CreatedAt":"2026-10-01 10:00:00 +0800 CST","ID":"sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","Repository":"nginx","Size":"187MB","Tag":"latest"}
{"Containers":"N/A","CreatedAt":"2026-10-01 10:00:00 +0800 CST","ID":"sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","Repository":"nginx","Size":"187MB","Tag":"1.27"}
{"Containers":"2","CreatedAt":"2026-09-01 10:00:00 +0800 CST","ID":"sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","Repository":"<none>","Size":"10MB","Tag":"<none>"}
"#;
        let rows = images(raw);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "c".repeat(64));
        assert_eq!(rows[0].repo, "nginx");
        assert_eq!(rows[0].tags, vec!["latest".to_string(), "1.27".to_string()]);
        assert!(!rows[0].dangling);
        assert!(rows[1].dangling);
        assert!(rows[1].tags.is_empty());
        assert_eq!(rows[1].containers, 2);
    }

    #[test]
    fn reads_info_and_counts() {
        let raw = r#"{"ServerVersion":"24.0.7","APIVersion":"1.43","Driver":"overlay2","DockerRootDir":"/var/lib/docker","ContainersRunning":3,"Containers":5,"Images":9,"RegistryConfig":{"Mirrors":["https://mirror.example.com"]}}"#;
        let (info, counts) = overview_parts(raw);
        assert_eq!(info.version, "24.0.7");
        assert_eq!(info.storage_driver, "overlay2");
        assert_eq!(info.mirrors, vec!["https://mirror.example.com".to_string()]);
        assert_eq!(counts.containers_running, 3);
        assert_eq!(counts.containers_total, 5);
        assert_eq!(counts.images, 9);
    }

    #[test]
    fn tolerates_missing_info_fields() {
        let (info, counts) = overview_parts("{}");
        assert_eq!(info.version, "");
        assert_eq!(counts.images, 0);
    }

    #[test]
    fn reads_storage_usage() {
        let raw = r#"{"Type":"Images","TotalCount":"9","Active":"2","Size":"1.2GB","Reclaimable":"500MB (41%)"}
{"Type":"Containers","TotalCount":"5","Active":"3","Size":"20MB","Reclaimable":"10MB (50%)"}
{"Type":"Local Volumes","TotalCount":"1","Active":"1","Size":"1GB","Reclaimable":"0B (0%)"}
{"Type":"Build Cache","TotalCount":"0","Active":"0","Size":"0B","Reclaimable":"0B"}
"#;
        let usage = df(raw).unwrap();
        assert_eq!(usage.images_size, "1.2GB");
        assert_eq!(usage.containers_size, "20MB");
        assert_eq!(usage.volumes_size, "1GB");
        assert_eq!(usage.build_cache_size, "0B");
        assert_eq!(usage.reclaimable, "500MB (41%)");
        assert!(df("").is_none());
    }

    #[test]
    fn reads_registry_search_rows() {
        let raw = r#"{"Name":"nginx","Description":"Official build","StarCount":"20000","IsOfficial":"true","IsAutomated":"false"}
{"Name":"bitnami/nginx","Description":"Bitnami","StarCount":"123","IsOfficial":"false","IsAutomated":"true"}
"#;
        let rows = search(raw);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "nginx");
        assert_eq!(rows[0].stars, 20000);
        assert!(rows[0].official);
        assert!(!rows[0].automated);
        assert!(rows[1].automated);
    }
}
