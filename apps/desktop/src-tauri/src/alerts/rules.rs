use crate::ssh::sampler::SampleDisk;

pub const RULE_CPU: &str = "cpu";
pub const RULE_MEMORY: &str = "memory";
pub const RULE_DISK: &str = "disk";
pub const RULE_LOAD: &str = "load";
pub const RULE_OFFLINE: &str = "offline";
pub const RULE_HOSTKEY: &str = "hostkey";
pub const RULE_LOGIN: &str = "login";
pub const RULE_LOGIN_SOURCE: &str = "login_source";

pub fn severity(rule_id: &str) -> &'static str {
    match rule_id {
        RULE_OFFLINE | RULE_HOSTKEY => "critical",
        RULE_LOGIN_SOURCE => "info",
        _ => "warning",
    }
}

pub fn label(rule_id: &str) -> &'static str {
    match rule_id {
        RULE_CPU => "CPU 使用率",
        RULE_MEMORY => "内存使用率",
        RULE_DISK => "磁盘使用率",
        RULE_LOAD => "系统负载",
        RULE_OFFLINE => "服务器离线",
        RULE_HOSTKEY => "主机指纹变更",
        RULE_LOGIN => "登录失败",
        RULE_LOGIN_SOURCE => "新登录来源",
        _ => "告警",
    }
}

pub fn is_resource(rule_id: &str) -> bool {
    matches!(rule_id, RULE_CPU | RULE_MEMORY | RULE_DISK | RULE_LOAD)
}

pub fn disk_breaches(disks: &[SampleDisk], threshold: u8) -> Vec<&SampleDisk> {
    disks
        .iter()
        .filter(|disk| disk.percent >= threshold as f64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_matches_spec() {
        assert_eq!(severity(RULE_CPU), "warning");
        assert_eq!(severity(RULE_MEMORY), "warning");
        assert_eq!(severity(RULE_DISK), "warning");
        assert_eq!(severity(RULE_LOAD), "warning");
        assert_eq!(severity(RULE_OFFLINE), "critical");
        assert_eq!(severity(RULE_HOSTKEY), "critical");
        assert_eq!(severity(RULE_LOGIN), "warning");
        assert_eq!(severity(RULE_LOGIN_SOURCE), "info");
    }

    #[test]
    fn only_resources_use_threshold_switch() {
        assert!(is_resource(RULE_CPU));
        assert!(is_resource(RULE_DISK));
        assert!(!is_resource(RULE_OFFLINE));
        assert!(!is_resource(RULE_LOGIN));
        assert_eq!(label(RULE_DISK), "磁盘使用率");
    }

    #[test]
    fn breaches_compare_against_threshold() {
        let disks = vec![
            SampleDisk {
                mount: "/".into(),
                percent: 89.9,
            },
            SampleDisk {
                mount: "/data".into(),
                percent: 90.0,
            },
        ];
        let found = disk_breaches(&disks, 90);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mount, "/data");
    }
}
