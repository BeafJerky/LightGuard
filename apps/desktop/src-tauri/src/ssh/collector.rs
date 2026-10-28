use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use super::dto::{DiskInfo, InterfaceInfo, ProcessInfo, ServerSnapshot};
use super::error::SshError;
use super::session::{now_iso, ClientHandler};
use super::sftp::path::shell_quote;

pub const COLLECT_SCRIPT: &str = r#"LC_ALL=C; export LC_ALL
echo '###LG:meta###'
echo "hostname=$(hostname 2>/dev/null || cat /proc/sys/kernel/hostname 2>/dev/null)"
. /etc/os-release 2>/dev/null
echo "os=${PRETTY_NAME:-unknown}"
echo "kernel=$(uname -r)"
echo "arch=$(uname -m)"
echo "timezone=$(date +%Z 2>/dev/null)"
echo "now=$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null)"
awk '{printf "uptime=%.2f\n", $1}' /proc/uptime 2>/dev/null
date -d "@$(( $(date +%s) - $(awk '{printf "%d", $1}' /proc/uptime) ))" '+boot=%Y-%m-%d %H:%M' 2>/dev/null
echo '###LG:cpu###'
sed -n 's/^model name[[:space:]]*:[[:space:]]*//p' /proc/cpuinfo 2>/dev/null | head -n 1 | sed 's/^/model=/'
echo "cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null)"
awk -F: '/cpu MHz/{printf "mhz=%.3f\n", $2; exit}' /proc/cpuinfo
grep '^cpu ' /proc/stat | sed 's/^/stat=/'
grep '^cpu[0-9]' /proc/stat | sed -n '1,16p' | sed 's/^cpu[0-9]* *//;s/^/core=/'
echo '###LG:mem###'
grep -E '^(MemTotal|MemAvailable|SwapTotal|SwapFree):' /proc/meminfo
echo '###LG:load###'
cat /proc/loadavg
echo '###LG:disk###'
df -PTk 2>/dev/null | tail -n +2
echo '###LG:inode###'
df -PTi 2>/dev/null | tail -n +2
echo '###LG:net###'
cat /proc/net/dev | tail -n +3
for d in /sys/class/net/*; do
  n=$(basename "$d")
  echo "iface=$n"
  echo "mac=$(cat "$d/address" 2>/dev/null)"
  echo "mtu=$(cat "$d/mtu" 2>/dev/null)"
  echo "speed=$(cat "$d/speed" 2>/dev/null)"
done
ip -o -4 addr show 2>/dev/null | sed 's/^/addr=/'
cat /proc/net/route | tail -n +2
grep -E '^nameserver' /etc/resolv.conf 2>/dev/null
echo '###LG:proc###'
echo "count=$(ps -e --no-headers 2>/dev/null | wc -l | tr -d ' ')"
ps -eo pid=,user=,pcpu=,pmem=,stat=,comm= --sort=-pcpu 2>/dev/null | head -n 60
echo '###LG:temp###'
for f in /sys/class/thermal/thermal_zone*/temp; do cat "$f" 2>/dev/null; done | sort -n | tail -n 1
"#;

#[derive(Debug, Clone, Default)]
pub struct CpuCounters {
    total: u64,
    idle: u64,
    cores: Vec<(u64, u64)>,
}

#[derive(Debug, Clone, Default)]
pub struct NetCounters {
    rx: u64,
    tx: u64,
}

#[derive(Debug, Default)]
pub struct CollectorState {
    last_cpu: Option<(Instant, CpuCounters)>,
    last_net: Option<(Instant, HashMap<String, NetCounters>)>,
}

impl CollectorState {
    pub fn has_cpu_baseline(&self) -> bool {
        self.last_cpu.is_some()
    }
}

pub async fn collect(
    handle: &russh::client::Handle<ClientHandler>,
    state: &mut CollectorState,
) -> Result<ServerSnapshot, SshError> {
    let command = format!("LC_ALL=C sh -c {}", shell_quote(COLLECT_SCRIPT));
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
                russh::ChannelMsg::Data { data } => output.extend_from_slice(&data),
                russh::ChannelMsg::ExtendedData { .. } => {}
                russh::ChannelMsg::ExitStatus { exit_status } if exit_status != 0 => {
                    return Err(SshError::collect(format!(
                        "远程采集命令退出码 {exit_status}"
                    )));
                }
                russh::ChannelMsg::Close | russh::ChannelMsg::Eof => break,
                _ => {}
            }
        }
        Ok::<Vec<u8>, SshError>(output)
    };

    let output = match tokio::time::timeout(Duration::from_secs(8), read).await {
        Ok(result) => result?,
        Err(_) => return Err(SshError::timeout("系统信息采集超时")),
    };
    if output.is_empty() {
        return Err(SshError::collect("服务器未返回采集结果"));
    }
    Ok(parse_snapshot(
        &String::from_utf8_lossy(&output),
        state,
        Instant::now(),
    ))
}

pub fn parse_snapshot(text: &str, state: &mut CollectorState, now: Instant) -> ServerSnapshot {
    let sections = split_sections(text);
    let mut snap = ServerSnapshot::default();
    if let Some(meta) = sections.get("meta") {
        parse_meta(meta, &mut snap);
    }
    if let Some(cpu) = sections.get("cpu") {
        parse_cpu(cpu, &mut snap, &mut state.last_cpu, now);
    }
    if let Some(mem) = sections.get("mem") {
        parse_mem(mem, &mut snap);
    }
    if let Some(load) = sections.get("load") {
        parse_load(load, &mut snap);
    }
    if let Some(disk) = sections.get("disk") {
        snap.disks = parse_disks(disk);
        let inodes = sections
            .get("inode")
            .map(|lines| parse_inodes(lines))
            .unwrap_or_default();
        for disk in &mut snap.disks {
            if let Some(percent) = inodes.get(&disk.mount) {
                disk.inode_percent = *percent;
            }
        }
        recompute_disks(&mut snap);
    }
    if let Some(net) = sections.get("net") {
        parse_net(net, &mut snap, &mut state.last_net, now);
    }
    if let Some(proc) = sections.get("proc") {
        let (processes, count) = parse_processes(proc);
        snap.processes = processes;
        snap.process_count = count;
    }
    if let Some(temp) = sections.get("temp") {
        snap.temp_c = temp
            .iter()
            .filter_map(|line| line.trim().parse::<f64>().ok())
            .fold(0.0, f64::max)
            / 1000.0;
    }
    if snap.sampled_at.is_empty() {
        snap.sampled_at = now_iso();
    }
    if snap.cpu_cores == 0 {
        snap.cpu_cores = snap.cpu_cores_load.len() as u32;
    }
    snap
}

fn split_sections(text: &str) -> HashMap<String, Vec<&str>> {
    let mut sections: HashMap<String, Vec<&str>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if let Some(name) = line
            .strip_prefix("###LG:")
            .and_then(|rest| rest.strip_suffix("###"))
        {
            current = Some(name.to_string());
            sections.entry(name.to_string()).or_default();
            continue;
        }
        if let Some(name) = &current {
            sections.entry(name.clone()).or_default().push(line);
        }
    }
    sections
}

fn kv<'a>(lines: &'a [&str], key: &str) -> Option<&'a str> {
    let prefix = format!("{key}=");
    lines
        .iter()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .map(str::trim)
}

fn parse_meta(lines: &[&str], snap: &mut ServerSnapshot) {
    snap.hostname = kv(lines, "hostname").unwrap_or("—").to_string();
    snap.os = kv(lines, "os")
        .unwrap_or("unknown")
        .trim_matches('"')
        .to_string();
    snap.kernel = kv(lines, "kernel").unwrap_or("—").to_string();
    snap.arch = kv(lines, "arch").unwrap_or("—").to_string();
    snap.timezone = kv(lines, "timezone").unwrap_or("—").to_string();
    snap.sampled_at = kv(lines, "now").unwrap_or("").to_string();
    snap.uptime_seconds = kv(lines, "uptime")
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| value as u64)
        .unwrap_or(0);
    snap.boot_time = kv(lines, "boot").unwrap_or("—").to_string();
    if snap.boot_time.is_empty() {
        snap.boot_time = "—".to_string();
    }
}

fn parse_cpu(
    lines: &[&str],
    snap: &mut ServerSnapshot,
    last: &mut Option<(Instant, CpuCounters)>,
    now: Instant,
) {
    snap.cpu_model = kv(lines, "model").unwrap_or("—").to_string();
    snap.cpu_cores = kv(lines, "cores")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    snap.cpu_freq = kv(lines, "mhz")
        .and_then(|value| value.parse::<f64>().ok())
        .map(|mhz| {
            if mhz >= 1000.0 {
                format!("{:.2} GHz", mhz / 1000.0)
            } else {
                format!("{mhz:.0} MHz")
            }
        })
        .unwrap_or_else(|| "—".to_string());

    let counters = parse_cpu_counters(lines);
    snap.cpu_cores_load = counters
        .cores
        .iter()
        .map(|(total, idle)| {
            if *total == 0 {
                0.0
            } else {
                ((*total).saturating_sub(*idle) as f64 / *total as f64 * 100.0)
                    .clamp(0.0, 100.0)
            }
        })
        .collect();

    if let Some((previous_at, previous)) = last.as_ref() {
        let elapsed = now.duration_since(*previous_at).as_secs_f64();
        if elapsed > 0.0 {
            snap.cpu_percent = cpu_percent(previous, &counters);
            for (index, current) in counters.cores.iter().enumerate() {
                if let Some(previous_core) = previous.cores.get(index) {
                    let pair = CpuCounters {
                        total: previous_core.0,
                        idle: previous_core.1,
                        cores: Vec::new(),
                    };
                    let current_pair = CpuCounters {
                        total: current.0,
                        idle: current.1,
                        cores: Vec::new(),
                    };
                    if let Some(value) = snap.cpu_cores_load.get_mut(index) {
                        *value = cpu_percent(&pair, &current_pair);
                    }
                }
            }
        }
    } else if counters.total > 0 {
        snap.cpu_percent = (counters.total.saturating_sub(counters.idle) as f64
            / counters.total as f64
            * 100.0)
            .clamp(0.0, 100.0);
    }
    *last = Some((now, counters));
    if snap.cpu_cores == 0 {
        snap.cpu_cores = snap.cpu_cores_load.len() as u32;
    }
}

fn parse_cpu_counters(lines: &[&str]) -> CpuCounters {
    let mut counters = CpuCounters::default();
    for line in lines {
        if let Some(values) = line.strip_prefix("stat=") {
            let (total, idle) = counters_from(values);
            counters.total = total;
            counters.idle = idle;
        } else if let Some(values) = line.strip_prefix("core=") {
            counters.cores.push(counters_from(values));
        }
    }
    counters
}

fn counters_from(values: &str) -> (u64, u64) {
    let mut numbers = values
        .split_whitespace()
        .filter_map(|value| value.parse::<u64>().ok());
    let mut total = 0u64;
    let mut idle = 0u64;
    for index in 0..8 {
        let value = numbers.next().unwrap_or(0);
        total = total.saturating_add(value);
        if index == 3 || index == 4 {
            idle = idle.saturating_add(value);
        }
    }
    (total, idle)
}

fn cpu_percent(previous: &CpuCounters, current: &CpuCounters) -> f64 {
    let delta_total = current.total.saturating_sub(previous.total);
    let delta_idle = current.idle.saturating_sub(previous.idle);
    if delta_total == 0 {
        return 0.0;
    }
    (delta_total.saturating_sub(delta_idle) as f64 / delta_total as f64 * 100.0)
        .clamp(0.0, 100.0)
}

fn parse_mem(lines: &[&str], snap: &mut ServerSnapshot) {
    let mut values = HashMap::new();
    for line in lines {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else { continue };
        let Some(value) = parts.next() else { continue };
        if let Ok(kb) = value.parse::<f64>() {
            values.insert(key.trim_end_matches(':').to_string(), kb);
        }
    }
    let gb = |key: &str| values.get(key).copied().unwrap_or(0.0) / 1_048_576.0;
    snap.mem_total_gb = gb("MemTotal");
    snap.mem_available_gb = gb("MemAvailable");
    snap.mem_percent = if snap.mem_total_gb > 0.0 {
        ((snap.mem_total_gb - snap.mem_available_gb) / snap.mem_total_gb * 100.0)
            .clamp(0.0, 100.0)
    } else {
        0.0
    };
    snap.swap_total_gb = gb("SwapTotal");
    let swap_free = gb("SwapFree");
    snap.swap_used_gb = (snap.swap_total_gb - swap_free).max(0.0);
}

fn parse_load(lines: &[&str], snap: &mut ServerSnapshot) {
    if let Some(line) = lines.iter().find(|line| !line.trim().is_empty()) {
        let mut values = line.split_whitespace();
        for index in 0..3 {
            snap.load[index] = values
                .next()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0);
        }
    }
}

fn parse_disks(lines: &[&str]) -> Vec<DiskInfo> {
    let mut disks = Vec::new();
    for line in lines {
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() < 7 {
            continue;
        }
        let fs = columns[0];
        let kind = columns[1];
        if is_pseudo_fs(fs, kind) {
            continue;
        }
        let total_kb = columns[2].parse::<f64>().unwrap_or(0.0);
        let used_kb = columns[3].parse::<f64>().unwrap_or(0.0);
        let mount = columns[6..].join(" ");
        disks.push(DiskInfo {
            mount,
            fs: fs.to_string(),
            kind: kind.to_string(),
            total_gb: total_kb / 1_048_576.0,
            used_gb: used_kb / 1_048_576.0,
            inode_percent: 0.0,
        });
    }
    if disks.is_empty() {
        for line in lines {
            let columns: Vec<&str> = line.split_whitespace().collect();
            if columns.len() >= 6 {
                let total_kb = columns[2].parse::<f64>().unwrap_or(0.0);
                let used_kb = columns[3].parse::<f64>().unwrap_or(0.0);
                disks.push(DiskInfo {
                    mount: columns[5..].join(" "),
                    fs: columns[0].to_string(),
                    kind: columns.get(1).unwrap_or(&"unknown").to_string(),
                    total_gb: total_kb / 1_048_576.0,
                    used_gb: used_kb / 1_048_576.0,
                    inode_percent: 0.0,
                });
            }
        }
    }
    disks
}

fn is_pseudo_fs(fs: &str, kind: &str) -> bool {
    matches!(
        kind,
        "tmpfs"
            | "devtmpfs"
            | "squashfs"
            | "overlay"
            | "proc"
            | "sysfs"
            | "cgroup"
            | "cgroup2"
            | "devpts"
            | "mqueue"
            | "nsfs"
            | "tracefs"
            | "debugfs"
            | "securityfs"
            | "pstore"
            | "configfs"
            | "fusectl"
            | "binfmt_misc"
            | "efivarfs"
            | "ramfs"
            | "autofs"
            | "rpc_pipefs"
            | "trimafs"
    ) || fs.starts_with("fuse.")
        || kind.starts_with("fuse")
        || fs.starts_with("/dev/loop")
}

fn parse_inodes(lines: &[&str]) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    for line in lines {
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() < 7 {
            continue;
        }
        let inode_total = columns[2].parse::<f64>().unwrap_or(0.0);
        if inode_total <= 0.0 {
            continue;
        }
        if let Ok(percent) = columns[5].trim_end_matches('%').parse::<f64>() {
            map.insert(columns[6..].join(" "), percent);
        }
    }
    map
}

fn recompute_disks(snap: &mut ServerSnapshot) {
    let mut total = 0.0;
    let mut used = 0.0;
    for disk in &snap.disks {
        total += disk.total_gb;
        used += disk.used_gb;
    }
    snap.disk_total_gb = total;
    snap.disk_percent = if total > 0.0 {
        (used / total * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
}

#[derive(Clone, Default)]
struct IfaceMeta {
    mac: String,
    mtu: String,
    speed: String,
    ip: String,
}

fn parse_net(
    lines: &[&str],
    snap: &mut ServerSnapshot,
    last: &mut Option<(Instant, HashMap<String, NetCounters>)>,
    now: Instant,
) {
    let mut counters: HashMap<String, NetCounters> = HashMap::new();
    let mut meta: BTreeMap<String, IfaceMeta> = BTreeMap::new();
    let mut gateway = String::new();
    let mut dns = Vec::new();
    let mut current_iface = String::new();

    for line in lines {
        if let Some((name, rest)) = line.split_once(':') {
            let name = name.trim();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || "._-@".contains(c)) {
                let values: Vec<u64> = rest
                    .split_whitespace()
                    .filter_map(|value| value.parse::<u64>().ok())
                    .collect();
                if values.len() >= 9 {
                    counters.insert(
                        name.to_string(),
                        NetCounters {
                            rx: values[0],
                            tx: values[8],
                        },
                    );
                    continue;
                }
            }
        }
        if let Some(name) = line.strip_prefix("iface=") {
            current_iface = name.trim().to_string();
            meta.entry(current_iface.clone()).or_default();
        } else if let Some(value) = line.strip_prefix("mac=") {
            if let Some(entry) = meta.get_mut(&current_iface) {
                entry.mac = value.trim().to_string();
            }
        } else if let Some(value) = line.strip_prefix("mtu=") {
            if let Some(entry) = meta.get_mut(&current_iface) {
                entry.mtu = value.trim().to_string();
            }
        } else if let Some(value) = line.strip_prefix("speed=") {
            if let Some(entry) = meta.get_mut(&current_iface) {
                let text = value.trim();
                let unknown = text.is_empty()
                    || text.starts_with('-')
                    || text == "4294967295";
                entry.speed = if unknown {
                    "—".to_string()
                } else {
                    format!("{text} Mbps")
                };
            }
        } else if let Some(value) = line.strip_prefix("addr=") {
            let columns: Vec<&str> = value.split_whitespace().collect();
            if let Some(index) = columns.iter().position(|column| *column == "inet") {
                if let Some(address) = columns.get(index + 1) {
                    let name = columns
                        .get(1)
                        .map(|name| name.trim_end_matches(':').to_string())
                        .or_else(|| columns.first().map(|name| name.to_string()));
                    if let Some(name) = name {
                        meta.entry(name.clone()).or_default().ip =
                            address.split('/').next().unwrap_or(address).to_string();
                    }
                }
            }
        } else if !line.starts_with("nameserver") {
            let columns: Vec<&str> = line.split_whitespace().collect();
            if columns.len() >= 3 && columns[1] == "00000000" && gateway.is_empty() {
                if let Some(value) = parse_gateway_hex(columns[2]) {
                    gateway = value;
                }
            }
        }
        if let Some(value) = line.strip_prefix("nameserver ") {
            dns.push(value.trim().to_string());
        }
    }

    // ip 命令的 addr 行以 "addr=" 开头；resolv.conf 行则直接以 nameserver 开头。
    let elapsed = last
        .as_ref()
        .map(|(previous_at, _)| now.duration_since(*previous_at).as_secs_f64())
        .unwrap_or(0.0);
    let previous = last.as_ref().map(|(_, map)| map);

    let mut down = 0.0;
    let mut up = 0.0;
    let mut interfaces = Vec::new();
    for (name, current) in &counters {
        if name == "lo" || is_container_iface(name) {
            continue;
        }
        let info = meta.get(name).cloned().unwrap_or_default();
        let mut down_mbps = 0.0;
        let mut up_mbps = 0.0;
        if let Some(previous_map) = previous {
            if let Some(previous_counters) = previous_map.get(name) {
                down_mbps = rate(previous_counters.rx, current.rx, elapsed);
                up_mbps = rate(previous_counters.tx, current.tx, elapsed);
                down += down_mbps;
                up += up_mbps;
            }
        }
        interfaces.push(InterfaceInfo {
            name: name.clone(),
            ip: if info.ip.is_empty() {
                "—".to_string()
            } else {
                info.ip
            },
            mac: if info.mac.is_empty() {
                "—".to_string()
            } else {
                info.mac
            },
            speed: if info.speed.is_empty() {
                "—".to_string()
            } else {
                info.speed
            },
            rx_total_gb: current.rx as f64 / 1_073_741_824.0,
            tx_total_gb: current.tx as f64 / 1_073_741_824.0,
            down_mbps,
            up_mbps,
        });
    }
    let addressed: Vec<InterfaceInfo> = interfaces
        .iter()
        .filter(|info| info.ip != "—")
        .cloned()
        .collect();
    if !addressed.is_empty() {
        interfaces = addressed;
    }
    interfaces.sort_by(|a, b| {
        let a_primary = a.ip != "—" && a.name != "lo";
        let b_primary = b.ip != "—" && b.name != "lo";
        b_primary.cmp(&a_primary).then_with(|| a.name.cmp(&b.name))
    });

    if let Some(primary) = interfaces.first() {
        snap.mtu = meta
            .get(&primary.name)
            .map(|info| info.mtu.clone())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "—".to_string());
        snap.public_ip = primary.ip.clone();
    } else {
        snap.mtu = "—".to_string();
        snap.public_ip = "—".to_string();
    }
    snap.gateway = if gateway.is_empty() {
        "—".to_string()
    } else {
        gateway
    };
    snap.dns = if dns.is_empty() {
        "—".to_string()
    } else {
        dns.join(", ")
    };
    snap.down_mbps = down;
    snap.up_mbps = up;
    snap.interfaces = interfaces;
    *last = Some((now, counters));
}

fn rate(previous_bytes: u64, current_bytes: u64, elapsed: f64) -> f64 {
    if elapsed <= 0.0 || current_bytes < previous_bytes {
        return 0.0;
    }
    (current_bytes - previous_bytes) as f64 / elapsed / 1_048_576.0
}

fn is_container_iface(name: &str) -> bool {
    name == "ovs-system"
        || name.starts_with("veth")
        || name.starts_with("docker")
        || name.starts_with("br-")
        || name.starts_with("virbr")
        || name.starts_with("vnet")
        || name.starts_with("tap")
        || name.starts_with("cni")
        || name.starts_with("flannel")
}

fn parse_gateway_hex(value: &str) -> Option<String> {
    if value.len() != 8 {
        return None;
    }
    let number = u32::from_str_radix(value, 16).ok()?;
    let bytes = number.to_le_bytes();
    Some(format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3]))
}

fn parse_processes(lines: &[&str]) -> (Vec<ProcessInfo>, u32) {
    let mut processes = Vec::new();
    let mut count = None;
    for line in lines {
        if let Some(value) = line.strip_prefix("count=") {
            count = value.trim().parse::<u32>().ok();
            continue;
        }
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() < 6 {
            continue;
        }
        let pid = match columns[0].parse::<u32>() {
            Ok(pid) => pid,
            Err(_) => continue,
        };
        processes.push(ProcessInfo {
            pid,
            user: columns[1].to_string(),
            cpu: columns[2].parse::<f64>().unwrap_or(0.0),
            mem: columns[3].parse::<f64>().unwrap_or(0.0),
            state: process_state(columns[4]),
            name: columns[5..].join(" "),
        });
    }
    let count = count.unwrap_or(processes.len() as u32);
    (processes, count)
}

fn process_state(raw: &str) -> String {
    match raw.chars().next().unwrap_or('?') {
        'R' => "运行",
        'S' => "睡眠",
        'D' => "等待",
        'Z' => "僵尸",
        'T' => "停止",
        'I' => "空闲",
        'X' => "退出",
        _ => "未知",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/collect.txt");

    #[test]
    fn parses_linux_snapshot() {
        let mut state = CollectorState::default();
        let snap = parse_snapshot(FIXTURE, &mut state, Instant::now());
        assert_eq!(snap.hostname, "test-host");
        assert!(snap.os.contains("Ubuntu"));
        assert_eq!(snap.cpu_cores, 4);
        assert!(snap.cpu_percent >= 0.0 && snap.cpu_percent <= 100.0);
        assert!(snap.mem_total_gb > 0.0);
        assert_eq!(snap.disks[0].mount, "/");
        assert!(snap.interfaces.iter().any(|iface| iface.name == "eth0"));
        assert_eq!(snap.processes[0].pid, 1);
        assert!(snap.cpu_cores_load.len() >= 2);
    }

    #[test]
    fn second_sample_computes_rates() {
        let mut state = CollectorState::default();
        let t0 = Instant::now();
        parse_snapshot(FIXTURE, &mut state, t0);
        let snap = parse_snapshot(FIXTURE, &mut state, t0 + Duration::from_secs(2));
        assert!(snap.down_mbps >= 0.0);
        assert!(snap.up_mbps >= 0.0);
    }

    #[test]
    fn missing_sections_do_not_panic() {
        let mut state = CollectorState::default();
        let snap = parse_snapshot("###LG:meta###\nhostname=mini\n", &mut state, Instant::now());
        assert_eq!(snap.hostname, "mini");
        assert!(snap.disks.is_empty());
        assert_eq!(snap.cpu_percent, 0.0);
    }

    #[test]
    fn gateway_is_little_endian() {
        assert_eq!(parse_gateway_hex("0100000A").as_deref(), Some("10.0.0.1"));
    }

    #[test]
    fn remote_and_virtual_mounts_are_excluded() {
        let lines = [
            "/dev/sda2 ext4 62914560 16777216 42949632 29% /",
            "efivarfs efivarfs 128 64 64 50% /sys/firmware/efi/efivars",
            "1000-1-cloud:storage fuse.rclone 1125899906842624 0 1125899906842624 0% /vol02/cloud",
            "trimafs trimafs 182452224 169869312 12582912 93% /fs",
            "/dev/sdb1 xfs 209715200 10485760 199229440 5% /data",
        ];
        let disks = parse_disks(&lines);
        let mounts: Vec<&str> = disks.iter().map(|disk| disk.mount.as_str()).collect();
        assert_eq!(mounts, vec!["/", "/data"]);

        let mut snap = ServerSnapshot::default();
        snap.disks = disks;
        recompute_disks(&mut snap);
        assert!((snap.disk_total_gb - 260.0).abs() < 0.01);
        assert!((snap.disk_percent - 10.0).abs() < 0.01);
    }

    #[test]
    fn inode_usage_ignores_filesystems_without_inodes() {
        let lines = [
            "/dev/sda2 ext4 1000 200 800 20% /",
            "/dev/sda1 vfat 0 0 0 - /boot/efi",
            "/dev/mapper/vol btrfs 0 0 0 - /vol1",
            "trimafs trimafs 10 10 0 100% /fs",
        ];
        let map = parse_inodes(&lines);
        assert_eq!(map.get("/"), Some(&20.0));
        assert!(!map.contains_key("/boot/efi"));
        assert!(!map.contains_key("/vol1"));
        assert_eq!(map.get("/fs"), Some(&100.0));
    }
}
