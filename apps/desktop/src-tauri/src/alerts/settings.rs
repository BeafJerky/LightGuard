use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_PERMIT_MINUTES: u8 = 15;
const THRESHOLD_RANGE: (u8, u8) = (50, 100);
const SUSTAIN_RANGE: (u32, u32) = (60, 3600);
const MULTIPLIER_RANGE: (f64, f64) = (1.0, 10.0);
const GRACE_RANGE: (u32, u32) = (30, 300);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ResourceRule {
    pub enabled: bool,
    pub threshold: u8,
    pub sustain_seconds: u32,
}

impl Default for ResourceRule {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 90,
            sustain_seconds: 180,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LoadRule {
    pub enabled: bool,
    pub multiplier: f64,
    pub sustain_seconds: u32,
}

impl Default for LoadRule {
    fn default() -> Self {
        Self {
            enabled: true,
            multiplier: 2.0,
            sustain_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OfflineRule {
    pub enabled: bool,
    pub grace_seconds: u32,
}

impl Default for OfflineRule {
    fn default() -> Self {
        Self {
            enabled: true,
            grace_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToggleRule {
    pub enabled: bool,
}

impl Default for ToggleRule {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AlertRules {
    pub cpu: ResourceRule,
    pub memory: ResourceRule,
    pub disk: ResourceRule,
    pub load: LoadRule,
    pub offline: OfflineRule,
    pub hostkey: ToggleRule,
    pub login: ToggleRule,
}

impl Default for AlertRules {
    fn default() -> Self {
        Self {
            cpu: ResourceRule::default(),
            memory: ResourceRule::default(),
            disk: ResourceRule {
                enabled: true,
                threshold: 90,
                sustain_seconds: 600,
            },
            load: LoadRule::default(),
            offline: OfflineRule::default(),
            hostkey: ToggleRule::default(),
            login: ToggleRule::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleOverride {
    pub cpu: Option<ResourceRule>,
    pub memory: Option<ResourceRule>,
    pub disk: Option<ResourceRule>,
    pub load: Option<LoadRule>,
    pub offline: Option<OfflineRule>,
    pub hostkey: Option<ToggleRule>,
    pub login: Option<ToggleRule>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AlertSettings {
    pub rules: AlertRules,
    pub per_server: HashMap<String, RuleOverride>,
}

#[derive(Debug, Clone)]
pub struct SettingsView {
    pub alerts: AlertSettings,
    pub permit_minutes: u8,
    pub notify_failure: bool,
    pub notify_threshold: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_settings_use_defaults() {
        let view = parse_settings(None);
        assert_eq!(view.permit_minutes, 15);
        assert!(view.notify_failure);
        assert!(view.notify_threshold);
        assert_eq!(view.alerts.rules.cpu.threshold, 90);
        assert_eq!(view.alerts.rules.cpu.sustain_seconds, 180);
        assert_eq!(view.alerts.rules.disk.sustain_seconds, 600);
        assert_eq!(view.alerts.rules.load.multiplier, 2.0);
        assert_eq!(view.alerts.rules.offline.grace_seconds, 60);
    }

    #[test]
    fn values_are_clamped_and_independently_defaulted() {
        let raw = json!({
            "permit": { "defaultMinutes": 7 },
            "notifyFailure": false,
            "alerts": {
                "rules": {
                    "cpu": { "threshold": 120, "sustainSeconds": 10 },
                    "load": { "multiplier": 0.2, "sustainSeconds": 99999 },
                    "offline": { "graceSeconds": 5 },
                    "login": { "enabled": false }
                }
            }
        });
        let view = parse_settings(Some(&raw));
        assert_eq!(view.permit_minutes, 15, "invalid minutes fall back");
        assert!(!view.notify_failure);
        assert!(view.notify_threshold);
        assert_eq!(view.alerts.rules.cpu.threshold, 100);
        assert_eq!(view.alerts.rules.cpu.sustain_seconds, 60);
        assert_eq!(view.alerts.rules.load.multiplier, 1.0);
        assert_eq!(view.alerts.rules.load.sustain_seconds, 3600);
        assert_eq!(view.alerts.rules.offline.grace_seconds, 30);
        assert!(!view.alerts.rules.login.enabled);
        assert!(view.alerts.rules.hostkey.enabled, "absent sections default");
    }

    #[test]
    fn permit_accepts_only_five_fifteen_thirty() {
        for (value, expected) in [(5, 5), (15, 15), (30, 30), (10, 15), (0, 15)] {
            let raw = json!({ "permit": { "defaultMinutes": value } });
            assert_eq!(parse_settings(Some(&raw)).permit_minutes, expected);
        }
    }

    #[test]
    fn per_server_override_wins_field_by_field() {
        let raw = json!({
            "alerts": {
                "rules": { "cpu": { "threshold": 80, "sustainSeconds": 180 } },
                "perServer": {
                    "srv_1": { "cpu": { "enabled": false, "threshold": 95, "sustainSeconds": 120 } }
                }
            }
        });
        let view = parse_settings(Some(&raw));
        let effective = view.alerts.effective("srv_1");
        assert!(!effective.cpu.enabled);
        assert_eq!(effective.cpu.threshold, 95);
        assert_eq!(effective.cpu.sustain_seconds, 120);
        assert_eq!(effective.memory.threshold, 90, "untouched rules stay global");

        let other = view.alerts.effective("srv_2");
        assert_eq!(other.cpu.threshold, 80);
        assert_eq!(other.cpu.sustain_seconds, 180);
    }

    #[test]
    fn garbage_settings_fall_back_to_defaults() {
        let raw = json!({ "alerts": "nope", "permit": 42, "notifyThreshold": "yes" });
        let view = parse_settings(Some(&raw));
        assert_eq!(view.alerts.rules.cpu.threshold, 90);
        assert_eq!(view.permit_minutes, 15);
        assert!(view.notify_threshold);
    }
}

fn clamp_range(value: impl Into<f64>, low: f64, high: f64) -> f64 {
    value.into().clamp(low, high)
}

fn clamp_resource(rule: ResourceRule) -> ResourceRule {
    ResourceRule {
        enabled: rule.enabled,
        threshold: clamp_range(
            rule.threshold,
            THRESHOLD_RANGE.0 as f64,
            THRESHOLD_RANGE.1 as f64,
        ) as u8,
        sustain_seconds: clamp_range(
            rule.sustain_seconds,
            SUSTAIN_RANGE.0 as f64,
            SUSTAIN_RANGE.1 as f64,
        ) as u32,
    }
}

fn clamp_load(rule: LoadRule) -> LoadRule {
    LoadRule {
        enabled: rule.enabled,
        multiplier: clamp_range(rule.multiplier, MULTIPLIER_RANGE.0, MULTIPLIER_RANGE.1),
        sustain_seconds: clamp_range(
            rule.sustain_seconds,
            SUSTAIN_RANGE.0 as f64,
            SUSTAIN_RANGE.1 as f64,
        ) as u32,
    }
}

fn clamp_offline(rule: OfflineRule) -> OfflineRule {
    OfflineRule {
        enabled: rule.enabled,
        grace_seconds: clamp_range(
            rule.grace_seconds,
            GRACE_RANGE.0 as f64,
            GRACE_RANGE.1 as f64,
        ) as u32,
    }
}

impl AlertRules {
    pub fn clamped(self) -> Self {
        Self {
            cpu: clamp_resource(self.cpu),
            memory: clamp_resource(self.memory),
            disk: clamp_resource(self.disk),
            load: clamp_load(self.load),
            offline: clamp_offline(self.offline),
            hostkey: self.hostkey,
            login: self.login,
        }
    }
}

impl RuleOverride {
    fn clamped(self) -> Self {
        Self {
            cpu: self.cpu.map(clamp_resource),
            memory: self.memory.map(clamp_resource),
            disk: self.disk.map(clamp_resource),
            load: self.load.map(clamp_load),
            offline: self.offline.map(clamp_offline),
            hostkey: self.hostkey,
            login: self.login,
        }
    }

    fn apply(&self, base: &mut AlertRules) {
        if let Some(rule) = &self.cpu {
            base.cpu = rule.clone();
        }
        if let Some(rule) = &self.memory {
            base.memory = rule.clone();
        }
        if let Some(rule) = &self.disk {
            base.disk = rule.clone();
        }
        if let Some(rule) = &self.load {
            base.load = rule.clone();
        }
        if let Some(rule) = &self.offline {
            base.offline = rule.clone();
        }
        if let Some(rule) = &self.hostkey {
            base.hostkey = rule.clone();
        }
        if let Some(rule) = &self.login {
            base.login = rule.clone();
        }
    }
}

impl AlertSettings {
    fn clamped(self) -> Self {
        Self {
            rules: self.rules.clamped(),
            per_server: self
                .per_server
                .into_iter()
                .take(500)
                .map(|(id, rule)| (id, rule.clamped()))
                .collect(),
        }
    }

    pub fn effective(&self, server_id: &str) -> AlertRules {
        let mut rules = self.rules.clone();
        if let Some(over) = self.per_server.get(server_id) {
            over.apply(&mut rules);
        }
        rules
    }
}

pub fn parse_settings(raw: Option<&Value>) -> SettingsView {
    let value = raw.cloned().unwrap_or(Value::Null);
    let alerts = value
        .get("alerts")
        .cloned()
        .and_then(|section| serde_json::from_value::<AlertSettings>(section).ok())
        .unwrap_or_default()
        .clamped();
    let permit_minutes = value
        .get("permit")
        .and_then(|section| section.get("defaultMinutes"))
        .and_then(Value::as_u64)
        .filter(|minutes| matches!(minutes, 5 | 15 | 30))
        .map(|minutes| minutes as u8)
        .unwrap_or(DEFAULT_PERMIT_MINUTES);
    let read_bool = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(true);
    SettingsView {
        alerts,
        permit_minutes,
        notify_failure: read_bool("notifyFailure"),
        notify_threshold: read_bool("notifyThreshold"),
    }
}
