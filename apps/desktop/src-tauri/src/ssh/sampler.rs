use std::time::{Duration, Instant};

use super::collector::{self, CollectorState};
use super::error::SshError;
use super::session::ClientHandler;
use super::sftp::path::shell_quote;

/// 只读轻量采样脚本：仅读取 `/proc` 与 `df`，无写入、无 sudo、无网络命令。
pub const SAMPLE_SCRIPT: &str = r#"LC_ALL=C; export LC_ALL
echo '###LG:load###'
cat /proc/loadavg 2>/dev/null
echo '###LG:cpu###'
echo "cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null)"
grep '^cpu ' /proc/stat 2>/dev/null | sed 's/^/stat=/'
echo '###LG:mem###'
grep -E '^(MemTotal|MemAvailable|SwapTotal|SwapFree):' /proc/meminfo 2>/dev/null
echo '###LG:disk###'
df -PTk 2>/dev/null | tail -n +2
"#;

pub const MAX_SAMPLE_BYTES: usize = 64 * 1024;
pub const SAMPLE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq)]
pub struct SampleDisk {
    pub mount: String,
    pub percent: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub cpu_percent: Option<f64>,
    pub mem_percent: f64,
    pub load1: f64,
    pub cores: u32,
    pub disks: Vec<SampleDisk>,
}

pub async fn exec_script(
    handle: &russh::client::Handle<ClientHandler>,
    script: &str,
    timeout: Duration,
    max_bytes: usize,
) -> Result<String, SshError> {
    let command = format!("LC_ALL=C sh -c {}", shell_quote(script));
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| SshError::collect(error.to_string()))?;
    channel
        .exec(true, command)
        .await
        .map_err(|error| SshError::collect(error.to_string()))?;

    let read = async {
        let mut output = Vec::new();
        while let Some(message) = channel.wait().await {
            match message {
                russh::ChannelMsg::Data { data } => {
                    output.extend_from_slice(&data);
                    if output.len() > max_bytes {
                        return Err(SshError::collect("远端脚本输出超过上限"));
                    }
                }
                russh::ChannelMsg::ExtendedData { .. } => {}
                russh::ChannelMsg::ExitStatus { exit_status } if exit_status != 0 => {
                    return Err(SshError::collect(format!("远端脚本退出码 {exit_status}")));
                }
                russh::ChannelMsg::Close | russh::ChannelMsg::Eof => break,
                _ => {}
            }
        }
        Ok::<Vec<u8>, SshError>(output)
    };

    let output = match tokio::time::timeout(timeout, read).await {
        Ok(result) => result?,
        Err(_) => return Err(SshError::timeout("远端脚本执行超时")),
    };
    if output.is_empty() {
        return Err(SshError::collect("服务器未返回脚本结果"));
    }
    Ok(String::from_utf8_lossy(&output).into_owned())
}

pub async fn collect_sample(
    handle: &russh::client::Handle<ClientHandler>,
    state: &mut CollectorState,
) -> Result<Sample, SshError> {
    let text = exec_script(handle, SAMPLE_SCRIPT, SAMPLE_TIMEOUT, MAX_SAMPLE_BYTES).await?;
    Ok(parse_sample(&text, state, Instant::now()))
}

/// 取出 `###LG:<name>###` 段的正文，直到下一个 `###LG:` 标记或文本结尾；正文去除首尾空白。
fn section<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("###LG:{name}###");
    let start = text.find(&marker)? + marker.len();
    let rest = &text[start..];
    let end = rest.find("###LG:").unwrap_or(rest.len());
    Some(rest[..end].trim())
}

pub fn parse_sample(text: &str, state: &mut CollectorState, now: Instant) -> Sample {
    let has_baseline = state.has_cpu_baseline();
    let snapshot = collector::parse_snapshot(text, state, now);
    // 缺失/不可读的读数以 NaN 表示“无信号”，避免被规则引擎当作真实的 0 而误判恢复。
    let mem_read = section(text, "mem").is_some_and(|body| !body.is_empty());
    let load_read = section(text, "load").is_some_and(|body| !body.is_empty());
    let cores = snapshot.cpu_cores;
    Sample {
        cpu_percent: if has_baseline {
            Some(snapshot.cpu_percent)
        } else {
            None
        },
        mem_percent: if mem_read {
            snapshot.mem_percent
        } else {
            f64::NAN
        },
        load1: if load_read && cores > 0 {
            snapshot.load[0]
        } else {
            f64::NAN
        },
        cores,
        disks: snapshot
            .disks
            .iter()
            .map(|disk| SampleDisk {
                mount: disk.mount.clone(),
                percent: if disk.total_gb > 0.0 {
                    (disk.used_gb / disk.total_gb * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                },
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(cpu_line: &str) -> String {
        format!(
            "###LG:load###\n2.00 1.00 0.50 1/100 1234\n\
             ###LG:cpu###\ncores=4\nstat={cpu_line}\n\
             ###LG:mem###\nMemTotal: 1000 kB\nMemAvailable: 500 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n\
             ###LG:disk###\n/dev/sda1 ext4 100000 90000 10000 90% /data\n"
        )
    }

    #[test]
    fn first_sample_has_no_cpu_percent() {
        let mut state = CollectorState::default();
        let sample = parse_sample(&fixture("cpu 100 0 100 800"), &mut state, Instant::now());
        assert_eq!(sample.cpu_percent, None);
        assert_eq!(sample.cores, 4);
        assert!((sample.mem_percent - 50.0).abs() < 0.01);
        assert!((sample.load1 - 2.0).abs() < 0.01);
        assert_eq!(sample.disks.len(), 1);
        assert_eq!(sample.disks[0].mount, "/data");
        assert!((sample.disks[0].percent - 90.0).abs() < 0.01);
    }

    #[test]
    fn second_sample_uses_counter_delta() {
        let mut state = CollectorState::default();
        let start = Instant::now();
        parse_sample(&fixture("cpu 100 0 100 800"), &mut state, start);
        let sample = parse_sample(
            &fixture("cpu 200 0 100 900"),
            &mut state,
            start + Duration::from_secs(10),
        );
        assert_eq!(sample.cpu_percent, Some(50.0));
    }

    #[test]
    fn missing_sections_yield_neutral_sample() {
        let mut state = CollectorState::default();
        let sample = parse_sample("###LG:mem###\n", &mut state, Instant::now());
        assert_eq!(sample.cpu_percent, None);
        assert!(sample.mem_percent.is_nan());
        assert!(sample.load1.is_nan());
        assert_eq!(sample.cores, 0);
        assert!(sample.disks.is_empty());
    }

    #[test]
    fn empty_mem_and_load_bodies_yield_nan_but_disks_parse() {
        let mut state = CollectorState::default();
        let text = "###LG:load###\n   \n\
                    ###LG:cpu###\ncores=4\nstat=cpu 100 0 100 800\n\
                    ###LG:mem###\n\n\
                    ###LG:disk###\n/dev/sda1 ext4 100000 90000 10000 90% /data\n";
        let sample = parse_sample(text, &mut state, Instant::now());
        assert!(sample.mem_percent.is_nan());
        assert!(sample.load1.is_nan());
        assert_eq!(sample.cores, 4);
        assert_eq!(sample.disks.len(), 1);
        assert_eq!(sample.disks[0].mount, "/data");
        assert!((sample.disks[0].percent - 90.0).abs() < 0.01);
    }

    #[test]
    fn complete_sample_keeps_finite_mem_and_load() {
        let mut state = CollectorState::default();
        let sample = parse_sample(&fixture("cpu 100 0 100 800"), &mut state, Instant::now());
        assert!(sample.mem_percent.is_finite());
        assert!(sample.load1.is_finite());
        assert!((sample.mem_percent - 50.0).abs() < 0.01);
        assert!((sample.load1 - 2.0).abs() < 0.01);
    }

    #[test]
    fn zero_cores_makes_present_load_neutral() {
        let mut state = CollectorState::default();
        let text = "###LG:load###\n2.00 1.00 0.50 1/100 1234\n\
                    ###LG:cpu###\nstat=cpu 100 0 100 800\n";
        let sample = parse_sample(text, &mut state, Instant::now());
        assert_eq!(sample.cores, 0);
        assert!(sample.load1.is_nan());
    }

    #[test]
    fn script_stays_read_only() {
        for forbidden in ["mkdir", "rmdir", "rm -", "touch", "chmod", "tee", "sudo"] {
            assert!(!SAMPLE_SCRIPT.contains(forbidden), "脚本不得包含 {forbidden}");
        }
    }
}
