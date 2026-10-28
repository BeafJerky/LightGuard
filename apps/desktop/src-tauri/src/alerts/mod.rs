pub mod engine;
pub mod notify;
pub mod rules;
pub mod settings;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use russh::client::Handle;
use tauri::AppHandle;
#[cfg(not(test))]
use tauri::Emitter;
use tokio::task::JoinHandle;

use crate::db::{Database, NewAlert};
use crate::ssh::collector::CollectorState;
use crate::ssh::sampler::{self, Sample};
use crate::ssh::session::ClientHandler;

use self::engine::{Engine, Transition};
use self::rules::{RULE_HOSTKEY, RULE_OFFLINE};
use self::settings::{parse_settings, SettingsView};

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

const SAMPLE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq)]
pub struct LoginStatus {
    pub available: bool,
    pub reason: String,
}

// 策略只看 severity：critical 跟随 `notifyFailure`，其余跟随 `notifyThreshold`。
// `rule_id` 保留在签名里供调用方统一传参，不参与判定。
pub(crate) fn should_notify(view: &SettingsView, rule_id: &str, severity: &str) -> bool {
    let _ = rule_id;
    if !view.notify_threshold && severity != "critical" {
        return false;
    }
    if !view.notify_failure && severity == "critical" {
        return false;
    }
    true
}

pub struct AlertService {
    db: Option<Arc<Database>>,
    // lib 测试二进制没有应用清单，而 `AppHandle` 的析构会把 tauri 平台层
    // （muda 的 comctl32 v6 导入）链进测试二进制，进程加载即 0xC0000139。
    // 测试从不注入句柄，因此测试构建不持有它；依赖它的副作用见 `announce`
    // 与 `notify_host` 的 cfg 分支。
    #[cfg(not(test))]
    app: Option<AppHandle>,
    settings: RwLock<SettingsView>,
    engine: Mutex<Engine>,
    samplers: Mutex<HashMap<String, JoinHandle<()>>>,
    offline: Mutex<HashMap<String, JoinHandle<()>>>,
    logins: Mutex<HashMap<String, JoinHandle<()>>>,
    login_status: Mutex<HashMap<String, LoginStatus>>,
    connected: Mutex<HashSet<String>>,
}

impl AlertService {
    pub(crate) fn new(db: Option<Arc<Database>>, app: Option<AppHandle>) -> Arc<Self> {
        // 测试构建不保存句柄；忘记而不是丢弃它，避免实例化 `AppHandle` 的析构胶水。
        #[cfg(test)]
        std::mem::forget(app);
        let service = Arc::new(Self {
            db,
            #[cfg(not(test))]
            app,
            settings: RwLock::new(parse_settings(None)),
            engine: Mutex::new(Engine::new()),
            samplers: Mutex::new(HashMap::new()),
            offline: Mutex::new(HashMap::new()),
            logins: Mutex::new(HashMap::new()),
            login_status: Mutex::new(HashMap::new()),
            connected: Mutex::new(HashSet::new()),
        });
        service.reload_settings();
        service
    }

    pub fn reload_settings(&self) {
        let raw = self
            .db
            .as_ref()
            .and_then(|db| db.settings_raw().ok().flatten());
        let mut view = parse_settings(raw.as_ref());
        if let Ok(threshold) = std::env::var("LG_TEST_ALERT_THRESHOLD") {
            if let Ok(threshold) = threshold.parse::<u8>() {
                view.alerts.rules.cpu.threshold = threshold;
                view.alerts.rules.cpu.sustain_seconds = 20;
            }
        }
        *write_lock(&self.settings) = view;
    }

    pub fn startup(&self) {
        if let Some(db) = &self.db {
            let _ = db.prune_retention(now_ms());
            let _ = db.alerts_resolve_all_active();
        }
        self.publish();
    }

    pub fn start(self: &Arc<Self>, server_id: String, handle: Arc<Handle<ClientHandler>>) {
        lock_map(&self.connected).insert(server_id.clone());
        self.stop_sampling(&server_id);
        self.cancel_offline(&server_id);
        self.resolve_rule(&server_id, RULE_OFFLINE);

        let service = Arc::clone(self);
        let id = server_id.clone();
        let task = tokio::spawn(async move {
            let server_name = service.server_name(&id);
            let mut state = CollectorState::default();
            let mut failures = 0u32;
            let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match sampler::collect_sample(&handle, &mut state).await {
                    Ok(sample) => {
                        failures = 0;
                        service.record_sample(&id, &server_name, &sample);
                    }
                    Err(_) => {
                        failures += 1;
                        if failures >= 3 {
                            break;
                        }
                    }
                }
            }
        });
        lock_map(&self.samplers).insert(server_id, task);
    }

    pub fn on_session_closed(self: &Arc<Self>, server_id: &str) {
        lock_map(&self.connected).remove(server_id);
        self.stop_sampling(server_id);
        let (enabled, grace) = {
            let settings = read_lock(&self.settings);
            let rules = settings.alerts.effective(server_id);
            (rules.offline.enabled, rules.offline.grace_seconds)
        };
        if enabled {
            self.schedule_offline(server_id.to_string(), Duration::from_secs(grace as u64));
        }
    }

    pub fn schedule_offline(self: &Arc<Self>, server_id: String, grace: Duration) {
        self.cancel_offline(&server_id);
        let service = Arc::clone(self);
        let id = server_id.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(grace).await;
            service.open_offline(&id, grace);
        });
        lock_map(&self.offline).insert(server_id, task);
    }

    pub fn on_explicit_disconnect(&self, server_id: &str) {
        lock_map(&self.connected).remove(server_id);
        self.stop_sampling(server_id);
        self.cancel_offline(server_id);
        lock_map(&self.engine).clear_server(server_id);
        if let Some(db) = &self.db {
            let _ = db.alerts_resolve_server(server_id);
        }
        self.publish();
    }

    pub fn on_host_key_mismatch(&self, server_id: &str, detail: &str) {
        let Some(db) = &self.db else {
            return;
        };
        let server_name = self.server_name(server_id);
        let event = NewAlert {
            server_id: server_id.to_string(),
            server_name: server_name.clone(),
            rule_id: RULE_HOSTKEY.to_string(),
            severity: "critical".to_string(),
            title: "主机指纹已变化".to_string(),
            detail: detail.to_string(),
            first_seen_at: now_ms(),
        };
        if db.alert_insert(&event).is_ok() {
            if should_notify(&read_lock(&self.settings), RULE_HOSTKEY, "critical") {
                self.notify_host("critical", &server_name, &event.title);
            }
            self.publish();
        }
    }

    pub fn reset(&self) {
        self.abort_all();
        lock_map(&self.connected).clear();
        lock_map(&self.engine).clear();
        lock_map(&self.login_status).clear();
        self.reload_settings();
        self.publish();
    }

    pub fn active_count(&self) -> i64 {
        self.db
            .as_ref()
            .and_then(|db| db.alerts_active_count().ok())
            .unwrap_or(0)
    }

    pub fn publish(&self) {
        let Some(db) = &self.db else {
            self.announce(0);
            return;
        };
        if let Ok(count) = db.alerts_active_count() {
            self.announce(count);
        }
    }

    #[cfg(not(test))]
    fn announce(&self, count: i64) {
        if let Some(app) = &self.app {
            let _ = app.emit("alerts://changed", serde_json::json!({ "activeCount": count }));
            notify::refresh_badge(app, count.max(0) as u32);
        }
    }

    #[cfg(test)]
    fn announce(&self, count: i64) {
        let _ = count;
    }

    #[cfg(not(test))]
    fn notify_host(&self, severity: &str, server_name: &str, title: &str) {
        if let Some(app) = &self.app {
            notify::notify_alert(app, severity, server_name, title);
        }
    }

    #[cfg(test)]
    fn notify_host(&self, _severity: &str, _server_name: &str, _title: &str) {}

    fn record_sample(&self, server_id: &str, server_name: &str, sample: &Sample) {
        let Some(db) = self.db.clone() else {
            return;
        };
        let rules = read_lock(&self.settings).alerts.effective(server_id);
        let mutes = db.alert_mutes_for(server_id).unwrap_or_default();
        let muted = |rule_id: &str| {
            mutes.iter().any(|mute| match &mute.rule_id {
                Some(id) => id == rule_id,
                None => true,
            })
        };
        let transitions = {
            let mut engine = lock_map(&self.engine);
            engine.step(server_id, sample, &rules, &muted)
        };
        for transition in transitions {
            match transition {
                Transition::Open {
                    rule_id,
                    severity,
                    title,
                    detail,
                } => {
                    let event = NewAlert {
                        server_id: server_id.to_string(),
                        server_name: server_name.to_string(),
                        rule_id: rule_id.clone(),
                        severity: severity.clone(),
                        title: title.clone(),
                        detail,
                        first_seen_at: now_ms(),
                    };
                    if let Err(error) = db.alert_insert(&event) {
                        eprintln!(
                            "[lightguard] alert insert failed for {server_id}/{rule_id}: {error}"
                        );
                        lock_map(&self.engine).forget(server_id, &rule_id);
                        continue;
                    }
                    let notify = should_notify(&read_lock(&self.settings), &rule_id, &severity);
                    if notify {
                        self.notify_host(&severity, server_name, &title);
                    }
                    self.publish();
                }
                Transition::Touch { rule_id, title } => {
                    match db.alert_active(server_id, &rule_id) {
                        Ok(Some(row)) => {
                            if let Err(error) = db.alert_touch(row.id, &title, now_ms()) {
                                eprintln!(
                                    "[lightguard] alert touch failed for {server_id}/{rule_id}: {error}"
                                );
                            }
                        }
                        Ok(None) => {}
                        Err(error) => eprintln!(
                            "[lightguard] alert lookup failed for {server_id}/{rule_id}: {error}"
                        ),
                    }
                }
                Transition::Resolve { rule_id } => self.resolve_rule(server_id, &rule_id),
            }
        }
    }

    fn resolve_rule(&self, server_id: &str, rule_id: &str) {
        let Some(db) = &self.db else {
            return;
        };
        match db.alert_active(server_id, rule_id) {
            Ok(Some(row)) => {
                if let Err(error) = db.alert_resolve(row.id) {
                    eprintln!(
                        "[lightguard] alert resolve failed for {server_id}/{rule_id}: {error}"
                    );
                }
                self.publish();
            }
            Ok(None) => {}
            Err(error) => eprintln!(
                "[lightguard] alert lookup failed for {server_id}/{rule_id}: {error}"
            ),
        }
    }

    fn open_offline(&self, server_id: &str, grace: Duration) {
        // 重连与定时器唤醒可能交错：会话若已恢复，这条离线告警就不该再开。
        let still_connected = lock_map(&self.connected).contains(server_id);
        if still_connected {
            return;
        }
        let Some(db) = &self.db else {
            return;
        };
        let (enabled, notify) = {
            let settings = read_lock(&self.settings);
            (
                settings.alerts.effective(server_id).offline.enabled,
                should_notify(&settings, RULE_OFFLINE, "critical"),
            )
        };
        if !enabled {
            return;
        }
        let server_name = self.server_name(server_id);
        let event = NewAlert {
            server_id: server_id.to_string(),
            server_name: server_name.clone(),
            rule_id: RULE_OFFLINE.to_string(),
            severity: "critical".to_string(),
            title: format!("服务器连接中断超过 {} 秒", grace.as_secs()),
            detail: String::new(),
            first_seen_at: now_ms(),
        };
        if db.alert_insert(&event).is_ok() {
            if notify {
                self.notify_host("critical", &server_name, &event.title);
            }
            self.publish();
        }
    }

    fn stop_sampling(&self, server_id: &str) {
        if let Some(task) = lock_map(&self.samplers).remove(server_id) {
            task.abort();
        }
    }

    fn cancel_offline(&self, server_id: &str) {
        if let Some(task) = lock_map(&self.offline).remove(server_id) {
            task.abort();
        }
    }

    pub fn abort_all(&self) {
        let samplers: Vec<JoinHandle<()>> =
            lock_map(&self.samplers).drain().map(|(_, task)| task).collect();
        for task in samplers {
            task.abort();
        }
        let logins: Vec<JoinHandle<()>> =
            lock_map(&self.logins).drain().map(|(_, task)| task).collect();
        for task in logins {
            task.abort();
        }
        let offline: Vec<JoinHandle<()>> =
            lock_map(&self.offline).drain().map(|(_, task)| task).collect();
        for task in offline {
            task.abort();
        }
    }

    pub fn server_name(&self, server_id: &str) -> String {
        self.db
            .as_ref()
            .and_then(|db| db.server_name(server_id).ok())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| server_id.to_string())
    }
}

fn lock_map<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn read_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poison| poison.into_inner())
}

fn write_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|poison| poison.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_settings() -> SettingsView {
        let mut view = parse_settings(None);
        view.alerts.rules.cpu.threshold = 90;
        view.alerts.rules.cpu.sustain_seconds = 10;
        view.alerts.rules.memory.enabled = false;
        view.alerts.rules.disk.enabled = false;
        view.alerts.rules.load.enabled = false;
        view
    }

    fn cpu(value: f64) -> Sample {
        Sample {
            cpu_percent: Some(value),
            mem_percent: 10.0,
            load1: 0.1,
            cores: 4,
            disks: Vec::new(),
        }
    }

    fn service_with(db: &Arc<Database>) -> Arc<AlertService> {
        let service = AlertService::new(Some(Arc::clone(db)), None);
        *service.settings.write().unwrap() = fast_settings();
        service
    }

    #[test]
    fn record_sample_opens_and_resolves_alert() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);

        service.record_sample("srv_1", "web-01", &cpu(95.0));
        assert_eq!(db.alerts_active_count().unwrap(), 1);
        let row = db.alert_active("srv_1", "cpu").unwrap().unwrap();
        assert_eq!(row.server_name, "web-01");
        assert_eq!(row.severity, "warning");

        service.record_sample("srv_1", "web-01", &cpu(10.0));
        service.record_sample("srv_1", "web-01", &cpu(10.0));
        assert_eq!(db.alerts_active_count().unwrap(), 1, "hysteresis holds");
        service.record_sample("srv_1", "web-01", &cpu(10.0));
        assert_eq!(db.alerts_active_count().unwrap(), 0, "three calm samples resolve");
    }

    #[test]
    fn muted_rule_stays_silent_until_mute_expires() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        db.alert_mute_add("srv_1", Some("cpu"), now_ms() + 60_000)
            .unwrap();
        for _ in 0..3 {
            service.record_sample("srv_1", "web-01", &cpu(95.0));
        }
        assert_eq!(db.alerts_active_count().unwrap(), 0);

        db.alert_mute_add("srv_1", Some("cpu"), now_ms() - 1).unwrap();
        service.record_sample("srv_1", "web-01", &cpu(95.0));
        assert_eq!(db.alerts_active_count().unwrap(), 1, "opens after unmute");
    }

    #[test]
    fn explicit_disconnect_resolves_active_alerts() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.record_sample("srv_1", "web-01", &cpu(95.0));
        assert_eq!(db.alerts_active_count().unwrap(), 1);

        lock_map(&service.connected).insert("srv_1".to_string());
        service.on_explicit_disconnect("srv_1");
        assert_eq!(db.alerts_active_count().unwrap(), 0);
        assert!(!lock_map(&service.connected).contains("srv_1"));
    }

    #[test]
    fn startup_resolves_leftover_active_alerts() {
        let db = Arc::new(Database::in_memory());
        db.alert_insert(&NewAlert {
            server_id: "srv_1".to_string(),
            server_name: "web-01".to_string(),
            rule_id: "cpu".to_string(),
            severity: "warning".to_string(),
            title: "CPU 使用率 95%".to_string(),
            detail: String::new(),
            first_seen_at: 1_000,
        })
        .unwrap();
        let service = AlertService::new(Some(Arc::clone(&db)), None);
        service.startup();
        assert_eq!(db.alerts_active_count().unwrap(), 0);
    }

    #[test]
    fn host_key_mismatch_creates_critical_alert() {
        let db = Arc::new(Database::in_memory());
        let service = AlertService::new(Some(Arc::clone(&db)), None);
        service.on_host_key_mismatch("srv_1", "SHA256:old -> SHA256:new");
        let row = db.alert_active("srv_1", "hostkey").unwrap().unwrap();
        assert_eq!(row.severity, "critical");
        assert!(row.detail.contains("SHA256:new"));
    }

    #[tokio::test]
    async fn offline_grace_opens_critical_alert() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.schedule_offline("srv_1".to_string(), Duration::from_millis(5));
        tokio::time::sleep(Duration::from_millis(80)).await;
        let row = db.alert_active("srv_1", "offline").unwrap().expect("offline alert");
        assert_eq!(row.severity, "critical");
        assert!(row.title.contains("连接中断"), "title was {}", row.title);
    }

    #[tokio::test]
    async fn offline_timer_skips_when_session_reconnected() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        lock_map(&service.connected).insert("srv_1".to_string());
        service.schedule_offline("srv_1".to_string(), Duration::from_millis(5));
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(
            db.alert_active("srv_1", "offline").unwrap().is_none(),
            "a reconnected session must not open an offline alert"
        );
    }

    #[tokio::test]
    async fn session_close_marks_server_offline_and_alerts() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        lock_map(&service.connected).insert("srv_1".to_string());
        write_lock(&service.settings).alerts.rules.offline.grace_seconds = 0;
        service.on_session_closed("srv_1");
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(!lock_map(&service.connected).contains("srv_1"));
        assert!(db.alert_active("srv_1", "offline").unwrap().is_some());
    }

    #[tokio::test]
    async fn offline_timer_respects_rule_disabled_at_fire_time() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.schedule_offline("srv_1".to_string(), Duration::from_millis(5));
        write_lock(&service.settings).alerts.rules.offline.enabled = false;
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(db.alert_active("srv_1", "offline").unwrap().is_none());
    }

    #[tokio::test]
    async fn offline_title_keeps_the_scheduled_grace() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.schedule_offline("srv_1".to_string(), Duration::from_millis(5));
        write_lock(&service.settings)
            .alerts
            .rules
            .offline
            .grace_seconds = 300;
        tokio::time::sleep(Duration::from_millis(80)).await;
        let row = db.alert_active("srv_1", "offline").unwrap().expect("offline alert");
        assert!(
            row.title.contains("超过 0 秒"),
            "title must use the grace captured at schedule time, was {}",
            row.title
        );
    }

    fn notify_view(notify_failure: bool, notify_threshold: bool) -> SettingsView {
        let mut view = parse_settings(None);
        view.notify_failure = notify_failure;
        view.notify_threshold = notify_threshold;
        view
    }

    #[test]
    fn should_notify_respects_failure_and_threshold_toggles() {
        assert!(
            !should_notify(&notify_view(false, true), "offline", "critical"),
            "critical alerts follow the failure toggle"
        );
        assert!(
            !should_notify(&notify_view(true, false), "cpu", "warning"),
            "threshold alerts follow the threshold toggle"
        );
        assert!(should_notify(&notify_view(true, true), "cpu", "warning"));
        assert!(should_notify(&notify_view(true, false), "offline", "critical"));
    }
}
