use std::collections::HashMap;

use crate::ssh::sampler::Sample;

use super::rules::{self, RULE_CPU, RULE_DISK, RULE_LOAD, RULE_MEMORY};
use super::settings::AlertRules;

#[derive(Debug, Clone, PartialEq)]
pub enum Transition {
    Open {
        rule_id: String,
        severity: String,
        title: String,
        detail: String,
    },
    Touch {
        rule_id: String,
        title: String,
    },
    Resolve {
        rule_id: String,
    },
}

#[derive(Debug, Default)]
pub struct Engine {
    states: HashMap<(String, String), RuleState>,
}

#[derive(Debug, Clone, Default)]
struct RuleState {
    streak: u32,
    calm: u32,
    opened: bool,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.states.clear();
    }

    pub fn clear_server(&mut self, server_id: &str) {
        self.states.retain(|(id, _), _| id != server_id);
    }
}

struct Signal {
    rule_id: &'static str,
    enabled: bool,
    value: Option<f64>,
    threshold: f64,
    recover_below: f64,
    need: u32,
    title: String,
    detail: String,
}

fn sustain_need(seconds: u32) -> u32 {
    (seconds / 10).max(1)
}

fn sustained(prefix: &str, value: f64, sustain_seconds: u32) -> String {
    let minutes = sustain_seconds / 60;
    if minutes >= 1 {
        format!("{prefix} {value:.0}%（持续 {minutes} 分钟）")
    } else {
        format!("{prefix} {value:.0}%")
    }
}

impl Engine {
    pub fn step(
        &mut self,
        server_id: &str,
        sample: &Sample,
        rules: &AlertRules,
        muted: &dyn Fn(&str) -> bool,
    ) -> Vec<Transition> {
        let mut out = Vec::new();
        // Task 7 契约：`cores == 0` 表示采样脚本的 CPU 段缺失或不可读，
        // 此时即便 `cpu_percent` 是 Some 也不可信，按“无 CPU 信号”处理。
        let cpu_value = if sample.cores == 0 {
            None
        } else {
            sample.cpu_percent
        };
        self.advance(
            &mut out,
            server_id,
            Signal {
                rule_id: RULE_CPU,
                enabled: rules.cpu.enabled,
                value: cpu_value,
                threshold: rules.cpu.threshold as f64,
                recover_below: rules.cpu.threshold as f64 - 5.0,
                need: sustain_need(rules.cpu.sustain_seconds),
                title: cpu_value
                    .map(|value| sustained("CPU 使用率", value, rules.cpu.sustain_seconds))
                    .unwrap_or_default(),
                detail: String::new(),
            },
            muted,
        );
        self.advance(
            &mut out,
            server_id,
            Signal {
                rule_id: RULE_MEMORY,
                enabled: rules.memory.enabled,
                value: Some(sample.mem_percent),
                threshold: rules.memory.threshold as f64,
                recover_below: rules.memory.threshold as f64 - 5.0,
                need: sustain_need(rules.memory.sustain_seconds),
                title: format!("内存使用率 {:.0}%", sample.mem_percent),
                detail: String::new(),
            },
            muted,
        );

        let worst = sample
            .disks
            .iter()
            .max_by(|a, b| a.percent.total_cmp(&b.percent));
        let breaches = rules::disk_breaches(&sample.disks, rules.disk.threshold);
        let detail = if breaches.is_empty() {
            String::new()
        } else {
            serde_json::to_string(
                &breaches
                    .iter()
                    .map(|disk| serde_json::json!({ "mount": disk.mount, "percent": disk.percent }))
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_default()
        };
        self.advance(
            &mut out,
            server_id,
            Signal {
                rule_id: RULE_DISK,
                enabled: rules.disk.enabled,
                value: worst.map(|disk| disk.percent),
                threshold: rules.disk.threshold as f64,
                recover_below: rules.disk.threshold as f64 - 5.0,
                need: sustain_need(rules.disk.sustain_seconds),
                title: worst
                    .map(|disk| format!("{} 使用率 {:.0}%", disk.mount, disk.percent))
                    .unwrap_or_default(),
                detail,
            },
            muted,
        );

        let cores = if sample.cores == 0 {
            1.0
        } else {
            sample.cores as f64
        };
        let load_threshold = cores * rules.load.multiplier;
        self.advance(
            &mut out,
            server_id,
            Signal {
                rule_id: RULE_LOAD,
                enabled: rules.load.enabled,
                value: Some(sample.load1),
                threshold: load_threshold,
                recover_below: load_threshold * 0.9,
                need: sustain_need(rules.load.sustain_seconds),
                title: format!("1 分钟负载 {:.1}（{} 核）", sample.load1, sample.cores.max(1)),
                detail: String::new(),
            },
            muted,
        );
        out
    }

    fn advance(
        &mut self,
        out: &mut Vec<Transition>,
        server_id: &str,
        signal: Signal,
        muted: &dyn Fn(&str) -> bool,
    ) {
        let key = (server_id.to_string(), signal.rule_id.to_string());
        if !signal.enabled {
            if let Some(state) = self.states.remove(&key) {
                if state.opened {
                    out.push(Transition::Resolve {
                        rule_id: signal.rule_id.to_string(),
                    });
                }
            }
            return;
        }
        let Some(value) = signal.value else {
            if let Some(state) = self.states.get_mut(&key) {
                state.streak = 0;
            }
            return;
        };
        let state = self.states.entry(key).or_default();
        if value >= signal.threshold {
            state.calm = 0;
            state.streak = state.streak.saturating_add(1);
            if state.streak < signal.need {
                return;
            }
            if !state.opened {
                if muted(signal.rule_id) {
                    return;
                }
                state.opened = true;
                out.push(Transition::Open {
                    rule_id: signal.rule_id.to_string(),
                    severity: rules::severity(signal.rule_id).to_string(),
                    title: signal.title,
                    detail: signal.detail,
                });
            } else {
                out.push(Transition::Touch {
                    rule_id: signal.rule_id.to_string(),
                    title: signal.title,
                });
            }
            return;
        }
        state.streak = 0;
        if value < signal.recover_below {
            state.calm = state.calm.saturating_add(1);
        } else {
            state.calm = 0;
        }
        if state.opened && state.calm >= 3 {
            state.opened = false;
            state.calm = 0;
            out.push(Transition::Resolve {
                rule_id: signal.rule_id.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::settings::{AlertRules, LoadRule, ResourceRule};
    use crate::ssh::sampler::{Sample, SampleDisk};

    fn sample(cpu: Option<f64>, mem: f64, load: f64, cores: u32, disks: Vec<SampleDisk>) -> Sample {
        Sample {
            cpu_percent: cpu,
            mem_percent: mem,
            load1: load,
            cores,
            disks,
        }
    }

    fn cpu_sample(value: f64) -> Sample {
        sample(Some(value), 10.0, 0.1, 4, Vec::new())
    }

    fn cpu_only_rules(sustain_seconds: u32) -> AlertRules {
        let mut rules = AlertRules::default();
        rules.cpu = ResourceRule {
            enabled: true,
            threshold: 90,
            sustain_seconds,
        };
        rules.memory.enabled = false;
        rules.disk.enabled = false;
        rules.load.enabled = false;
        rules
    }

    fn unmuted(_: &str) -> bool {
        false
    }

    #[test]
    fn opens_after_sustain_samples() {
        let rules = cpu_only_rules(60);
        let mut engine = Engine::new();
        for _ in 0..5 {
            assert!(engine
                .step("srv", &cpu_sample(95.0), &rules, &unmuted)
                .is_empty());
        }
        let events = engine.step("srv", &cpu_sample(95.0), &rules, &unmuted);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Transition::Open {
                rule_id,
                severity,
                title,
                ..
            } => {
                assert_eq!(rule_id, "cpu");
                assert_eq!(severity, "warning");
                assert!(title.contains("95%"), "title was {title}");
            }
            other => panic!("expected open, got {other:?}"),
        }
    }

    #[test]
    fn hysteresis_needs_three_calm_samples() {
        let rules = cpu_only_rules(10);
        let mut engine = Engine::new();
        engine.step("srv", &cpu_sample(95.0), &rules, &unmuted);
        assert!(
            engine
                .step("srv", &cpu_sample(88.0), &rules, &unmuted)
                .is_empty(),
            "88% is above threshold-5 and must keep the alert open"
        );
        engine.step("srv", &cpu_sample(80.0), &rules, &unmuted);
        engine.step("srv", &cpu_sample(80.0), &rules, &unmuted);
        let events = engine.step("srv", &cpu_sample(80.0), &rules, &unmuted);
        assert_eq!(events, vec![Transition::Resolve { rule_id: "cpu".into() }]);
    }

    #[test]
    fn mute_suppresses_open_until_unmuted() {
        let rules = cpu_only_rules(10);
        let mut engine = Engine::new();
        let muted = |rule_id: &str| rule_id == "cpu";
        for _ in 0..3 {
            assert!(engine
                .step("srv", &cpu_sample(95.0), &rules, &muted)
                .is_empty());
        }
        let events = engine.step("srv", &cpu_sample(95.0), &rules, &unmuted);
        assert!(matches!(events.as_slice(), [Transition::Open { .. }]));
    }

    #[test]
    fn disabling_rule_resolves_open_alert() {
        let mut rules = cpu_only_rules(10);
        let mut engine = Engine::new();
        engine.step("srv", &cpu_sample(95.0), &rules, &unmuted);
        rules.cpu.enabled = false;
        let events = engine.step("srv", &cpu_sample(95.0), &rules, &unmuted);
        assert_eq!(events, vec![Transition::Resolve { rule_id: "cpu".into() }]);
    }

    #[test]
    fn missing_cpu_resets_streak() {
        let rules = cpu_only_rules(30);
        let mut engine = Engine::new();
        engine.step("srv", &cpu_sample(95.0), &rules, &unmuted);
        engine.step("srv", &cpu_sample(95.0), &rules, &unmuted);
        assert!(engine
            .step(
                "srv",
                &sample(None, 10.0, 0.1, 4, Vec::new()),
                &rules,
                &unmuted
            )
            .is_empty());
        assert!(
            engine
                .step("srv", &cpu_sample(95.0), &rules, &unmuted)
                .is_empty(),
            "streak must restart after a sample without CPU data"
        );
    }

    #[test]
    fn zero_cores_tick_carries_no_cpu_signal() {
        // Task 7 契约：`cores == 0` 表示 CPU 段缺失/不可读，此时 cpu_percent 不可信。
        let rules = cpu_only_rules(10);
        let mut engine = Engine::new();
        assert!(matches!(
            engine.step("srv", &cpu_sample(95.0), &rules, &unmuted).as_slice(),
            [Transition::Open { .. }]
        ));
        assert!(
            engine
                .step(
                    "srv",
                    &sample(Some(20.0), 10.0, 0.1, 0, Vec::new()),
                    &rules,
                    &unmuted,
                )
                .is_empty(),
            "a tick without a CPU reading must not resolve the open alert"
        );
        for _ in 0..3 {
            assert!(
                engine
                    .step(
                        "srv",
                        &sample(Some(99.0), 10.0, 0.1, 0, Vec::new()),
                        &rules,
                        &unmuted,
                    )
                    .is_empty(),
                "a tick without a CPU reading must not open or touch the alert"
            );
        }
        // 状态仍在：读数恢复后同一告警继续以 Touch 推进，而不是重新 Open。
        assert!(matches!(
            engine.step("srv", &cpu_sample(99.0), &rules, &unmuted).as_slice(),
            [Transition::Touch { .. }]
        ));
    }

    #[test]
    fn disk_alert_uses_worst_mount_and_json_detail() {
        let mut rules = AlertRules::default();
        rules.cpu.enabled = false;
        rules.memory.enabled = false;
        rules.load.enabled = false;
        rules.disk = ResourceRule {
            enabled: true,
            threshold: 90,
            sustain_seconds: 10,
        };
        let mut engine = Engine::new();
        let disks = vec![
            SampleDisk {
                mount: "/".into(),
                percent: 50.0,
            },
            SampleDisk {
                mount: "/data".into(),
                percent: 95.0,
            },
            SampleDisk {
                mount: "/backup".into(),
                percent: 91.0,
            },
        ];
        let events = engine.step(
            "srv",
            &sample(Some(1.0), 10.0, 0.1, 4, disks),
            &rules,
            &unmuted,
        );
        match &events[0] {
            Transition::Open {
                rule_id,
                title,
                detail,
                ..
            } => {
                assert_eq!(rule_id, "disk");
                assert!(title.starts_with("/data 使用率 95%"), "title was {title}");
                assert!(detail.contains("/backup"));
                assert_eq!(detail.matches("\"mount\"").count(), 2, "detail was {detail}");
            }
            other => panic!("expected open, got {other:?}"),
        }
    }

    #[test]
    fn load_threshold_scales_with_cores() {
        let mut rules = AlertRules::default();
        rules.cpu.enabled = false;
        rules.memory.enabled = false;
        rules.disk.enabled = false;
        rules.load = LoadRule {
            enabled: true,
            multiplier: 2.0,
            sustain_seconds: 10,
        };
        let mut engine = Engine::new();
        let events = engine.step(
            "srv",
            &sample(Some(1.0), 10.0, 8.5, 4, Vec::new()),
            &rules,
            &unmuted,
        );
        assert!(matches!(events.as_slice(), [Transition::Open { .. }]));
    }
}
