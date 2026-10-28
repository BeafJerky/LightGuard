pub mod commands;
pub mod engine;
pub mod login;
pub mod notify;
pub mod rules;
pub mod settings;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use russh::client::Handle;
use tauri::AppHandle;
#[cfg(not(test))]
use tauri::Emitter;
use tokio::task::JoinHandle;

use crate::db::{AlertFilter, AlertRow, Database, NewAlert};
use crate::ssh::collector::CollectorState;
use crate::ssh::error::SshError;
use crate::ssh::sampler::{self, Sample};
use crate::ssh::session::ClientHandler;

use self::engine::{Engine, Transition};
use self::login::LoginReport;
use self::rules::{RULE_HOSTKEY, RULE_LOGIN, RULE_LOGIN_SOURCE, RULE_OFFLINE};
use self::settings::{parse_settings, SettingsView};

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

const SAMPLE_INTERVAL: Duration = Duration::from_secs(10);

/// 保留裁剪只在启动时跑一次会一直拖到重启；采样路径上按小时限频补跑。
const PRUNE_INTERVAL_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct LoginStatus {
    pub available: bool,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginMonitor {
    pub server_id: String,
    pub available: bool,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertsPage {
    pub items: Vec<crate::db::AlertRow>,
    pub active_count: i64,
    pub login_monitor: Vec<LoginMonitor>,
}

// 提醒分类按规则而非仅按 severity：新登录来源是信息级、只进告警中心不弹通知；
// 登录失败跟随 `notifyFailure`；critical 与其余阈值类规则仍按各自开关判定。
pub(crate) fn should_notify(view: &SettingsView, rule_id: &str, severity: &str) -> bool {
    if rule_id == RULE_LOGIN_SOURCE {
        return false;
    }
    if severity == "critical" || rule_id == RULE_LOGIN {
        return view.notify_failure;
    }
    view.notify_threshold
}

/// 登录失败告警与引擎路径一致：`alert_insert` 对活跃行是就地更新（返回同一 id），
/// 所以只有"此前没有活跃行"才是这波失败的第一个上升沿，其余轮次只刷新标题与计数。
fn login_burst_is_new(previous_active: Option<&AlertRow>) -> bool {
    previous_active.is_none()
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
    login_baseline: Mutex<HashSet<String>>,
    connected: Mutex<HashSet<String>>,
    /// 上次保留裁剪的时刻（毫秒），只在采样写入路径上做无锁限频。
    last_prune: AtomicI64,
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
            login_baseline: Mutex::new(HashSet::new()),
            connected: Mutex::new(HashSet::new()),
            last_prune: AtomicI64::new(0),
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
            let now = now_ms();
            if let Err(error) = db.prune_retention(now) {
                eprintln!("[lightguard] retention prune failed: {error}");
            }
            self.last_prune.store(now, Ordering::Relaxed);
            let _ = db.alerts_resolve_all_active();
        }
        self.publish();
    }

    /// 采样写入后的补跑：限频一小时，失败只记日志，不阻塞采样。
    fn prune_if_due(&self) {
        let Some(db) = &self.db else {
            return;
        };
        let now = now_ms();
        let last = self.last_prune.load(Ordering::Relaxed);
        if now - last < PRUNE_INTERVAL_MS {
            return;
        }
        self.last_prune.store(now, Ordering::Relaxed);
        if let Err(error) = db.prune_retention(now) {
            eprintln!("[lightguard] retention prune failed: {error}");
        }
    }

    pub fn start(self: &Arc<Self>, server_id: String, handle: Arc<Handle<ClientHandler>>) {
        lock_map(&self.connected).insert(server_id.clone());
        self.stop_sampling(&server_id);
        self.cancel_offline(&server_id);
        self.resolve_rule(&server_id, RULE_OFFLINE);

        let login_handle = Arc::clone(&handle);
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
                    Err(error) => {
                        // 采样失败不等于会话结束：ticker 自带 10 秒节拍，继续重试即可；
                        // 只有 stop_sampling/abort 才该结束这个循环。
                        failures += 1;
                        if failures == 3 {
                            eprintln!("[lightguard] sampling failed {failures}x for {id}: {error}");
                        }
                    }
                }
            }
        });
        self.start_login(&server_id, login_handle);
        lock_map(&self.samplers).insert(server_id, task);
    }

    pub fn on_session_closed(self: &Arc<Self>, server_id: &str) {
        lock_map(&self.connected).remove(server_id);
        self.stop_sampling(server_id);
        self.stop_login(server_id);
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
        self.stop_login(server_id);
        self.cancel_offline(server_id);
        lock_map(&self.engine).clear_server(server_id);
        if let Some(db) = &self.db {
            let _ = db.alerts_resolve_server(server_id);
        }
        self.publish();
    }

    pub fn on_host_key_mismatch(&self, server_id: &str, detail: &str) {
        // 没有数据库时既写不了新行也收口不了旧行，先于设置读取退出。
        let Some(db) = &self.db else {
            return;
        };
        // 与 `ingest_login` 同构：规则关闭时只收口已有告警，不插入也不弹通知。
        // 设置锁放在独立作用域里，先释放再走 `resolve_rule`（它要查库并发布），
        // 不让读锁跨过别的锁与副作用，避免与引擎/发布路径形成反向获取顺序。
        let (enabled, notify) = {
            let settings = read_lock(&self.settings);
            (
                settings.alerts.effective(server_id).hostkey.enabled,
                should_notify(&settings, RULE_HOSTKEY, "critical"),
            )
        };
        if !enabled {
            self.resolve_rule(server_id, RULE_HOSTKEY);
            return;
        }
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
            if notify {
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
        lock_map(&self.login_baseline).clear();
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
        let wrote = !transitions.is_empty();
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
                        // 引擎仍处 opened 态（例如这条告警被忽略），没有活跃行时可
                        // 能在忽略窗口到期后重开；静音期间保持沉默，与 Open 一致。
                        Ok(None) => {
                            if muted(&rule_id) {
                                continue;
                            }
                            match db.alert_reopen_ignored(server_id, &rule_id, &title, now_ms()) {
                                Ok(false) => {}
                                Ok(true) => {
                                    let severity = rules::severity(&rule_id).to_string();
                                    let notify = should_notify(
                                        &read_lock(&self.settings),
                                        &rule_id,
                                        &severity,
                                    );
                                    if notify {
                                        self.notify_host(&severity, server_name, &title);
                                    }
                                    self.publish();
                                }
                                Err(error) => eprintln!(
                                    "[lightguard] alert reopen failed for {server_id}/{rule_id}: {error}"
                                ),
                            }
                        }
                        Err(error) => eprintln!(
                            "[lightguard] alert lookup failed for {server_id}/{rule_id}: {error}"
                        ),
                    }
                }
                Transition::Resolve { rule_id } => self.resolve_rule(server_id, &rule_id),
            }
        }
        if wrote {
            self.prune_if_due();
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

    fn start_login(self: &Arc<Self>, server_id: &str, handle: Arc<Handle<ClientHandler>>) {
        if let Some(task) = lock_map(&self.logins).remove(server_id) {
            task.abort();
        }
        let service = Arc::clone(self);
        let id = server_id.to_string();
        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match sampler::exec_script(
                    &handle,
                    login::LOGIN_SCRIPT,
                    Duration::from_secs(10),
                    256 * 1024,
                )
                .await
                {
                    Ok(text) => service.ingest_login(&id, &login::parse_login(&text)),
                    Err(_) => service.mark_login_unavailable(&id),
                }
            }
        });
        lock_map(&self.logins).insert(server_id.to_string(), task);
    }

    fn stop_login(&self, server_id: &str) {
        if let Some(task) = lock_map(&self.logins).remove(server_id) {
            task.abort();
        }
        // 监控已停：状态表里不能再留着这台机器，否则已断开的服务器仍在"登录监控"里挂着。
        lock_map(&self.login_status).remove(server_id);
    }

    fn mark_login_unavailable(&self, server_id: &str) {
        let changed = {
            let mut statuses = lock_map(&self.login_status);
            let next = LoginStatus {
                available: false,
                reason: "无法读取登录日志（只读解析）".to_string(),
            };
            if statuses.get(server_id) == Some(&next) {
                false
            } else {
                statuses.insert(server_id.to_string(), next);
                true
            }
        };
        if changed {
            self.publish();
        }
    }

    fn ingest_login(&self, server_id: &str, report: &LoginReport) {
        let Some(db) = self.db.clone() else {
            return;
        };
        let status_changed = {
            let mut statuses = lock_map(&self.login_status);
            let next = LoginStatus {
                available: report.available,
                reason: if report.available {
                    report.source.clone()
                } else {
                    "未检测到登录日志（只读解析）".to_string()
                },
            };
            let changed = statuses.get(server_id) != Some(&next);
            statuses.insert(server_id.to_string(), next);
            changed
        };
        if !report.available {
            if status_changed {
                self.publish();
            }
            return;
        }

        let baseline = {
            let mut seen = lock_map(&self.login_baseline);
            if seen.contains(server_id) {
                true
            } else {
                seen.insert(server_id.to_string());
                false
            }
        };
        let server_name = self.server_name(server_id);
        for ip in &report.accepted {
            // 首个报告只用来建立基线：先入库来源，再决定是否告警，避免开屏即刷屏。
            let first = db.login_source_touch(server_id, ip, now_ms()).unwrap_or(false);
            if baseline && first {
                let event = NewAlert {
                    server_id: server_id.to_string(),
                    server_name: server_name.clone(),
                    rule_id: RULE_LOGIN_SOURCE.to_string(),
                    severity: "info".to_string(),
                    title: format!("新的登录来源 {ip}"),
                    detail: String::new(),
                    first_seen_at: now_ms(),
                };
                if db.alert_insert(&event).is_ok() {
                    if should_notify(&read_lock(&self.settings), RULE_LOGIN_SOURCE, "info") {
                        self.notify_host("info", &server_name, &event.title);
                    }
                    self.publish();
                }
            }
        }
        if baseline && status_changed {
            self.publish();
        }
        if !read_lock(&self.settings).alerts.effective(server_id).login.enabled {
            self.resolve_rule(server_id, RULE_LOGIN);
            return;
        }
        if !baseline {
            return;
        }
        if report.failed >= 5 {
            // 判定必须在插入之前：插入对活跃行是就地更新，插入后再查只会得到"已存在"。
            let rising_edge = match db.alert_active(server_id, RULE_LOGIN) {
                Ok(previous) => login_burst_is_new(previous.as_ref()),
                Err(error) => {
                    eprintln!(
                        "[lightguard] alert lookup failed for {server_id}/{RULE_LOGIN}: {error}"
                    );
                    // 查不到状态时按"已在告警中"处理：宁可少弹一次，也不要每 60 秒重复刷屏。
                    false
                }
            };
            let event = NewAlert {
                server_id: server_id.to_string(),
                server_name: server_name.clone(),
                rule_id: RULE_LOGIN.to_string(),
                severity: "warning".to_string(),
                title: format!("最近日志中失败登录 {} 次", report.failed),
                detail: String::new(),
                first_seen_at: now_ms(),
            };
            // 每轮都插入（就地更新）让标题里的次数保持新鲜，但只有上升沿才弹通知。
            if db.alert_insert(&event).is_ok() {
                let announce =
                    rising_edge && should_notify(&read_lock(&self.settings), RULE_LOGIN, "warning");
                if announce {
                    self.notify_host("warning", &server_name, &event.title);
                }
                self.publish();
            }
        } else {
            self.resolve_rule(server_id, RULE_LOGIN);
        }
    }

    pub fn list_alerts(&self, filter: &str, limit: i64, offset: i64) -> Result<AlertsPage, SshError> {
        let Some(db) = &self.db else {
            return Err(SshError::internal("本地数据库不可用"));
        };
        let statuses: &'static [&'static str] = match filter {
            "open" => &["active"],
            "handled" => &["resolved", "ignored"],
            _ => &[],
        };
        let items = db
            .alerts_list(&AlertFilter {
                statuses,
                limit: limit.clamp(1, 200),
                offset: offset.max(0),
            })
            .map_err(|error| SshError::internal(error.to_string()))?;
        let active_count = db
            .alerts_active_count()
            .map_err(|error| SshError::internal(error.to_string()))?;
        let mut login_monitor: Vec<LoginMonitor> = lock_map(&self.login_status)
            .iter()
            .map(|(server_id, status)| LoginMonitor {
                server_id: server_id.clone(),
                available: status.available,
                reason: status.reason.clone(),
            })
            .collect();
        login_monitor.sort_by(|a, b| a.server_id.cmp(&b.server_id));
        Ok(AlertsPage {
            items,
            active_count,
            login_monitor,
        })
    }

    pub fn ignore_alert(&self, alert_id: i64, reason: &str, until_ms: i64) -> Result<(), SshError> {
        let reason = reason.trim();
        if reason.is_empty() || reason.chars().count() > 200 {
            return Err(SshError::invalid_input("忽略原因需为 1–200 个字符"));
        }
        validate_until(until_ms)?;
        let Some(db) = &self.db else {
            return Err(SshError::internal("本地数据库不可用"));
        };
        db.alert_ignore(alert_id, reason, until_ms)
            .map_err(|error| SshError::internal(error.to_string()))?;
        self.publish();
        Ok(())
    }

    pub fn mute_server(
        &self,
        server_id: &str,
        rule_id: Option<&str>,
        until_ms: i64,
    ) -> Result<(), SshError> {
        validate_until(until_ms)?;
        let Some(db) = &self.db else {
            return Err(SshError::internal("本地数据库不可用"));
        };
        db.alert_mute_add(server_id, rule_id, until_ms)
            .map_err(|error| SshError::internal(error.to_string()))?;
        // 静音会改变后续告警的可见性，让打开着的告警列表立刻刷新一次。
        self.publish();
        Ok(())
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

fn validate_until(until_ms: i64) -> Result<(), SshError> {
    let now = now_ms();
    if until_ms <= now || until_ms > now + 7 * 24 * 60 * 60 * 1000 {
        return Err(SshError::invalid_input("忽略/静音时长需在 1 分钟到 7 天之间"));
    }
    Ok(())
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
        assert_eq!(db.alerts_active_count().unwrap(), 1);
    }

    fn all_alerts(db: &Arc<Database>) -> Vec<crate::db::AlertRow> {
        db.alerts_list(&AlertFilter {
            statuses: &[],
            limit: 50,
            offset: 0,
        })
        .unwrap()
    }

    #[test]
    fn disabled_hostkey_rule_stays_silent_and_resolves_the_open_row() {
        // 通知开关保持全开：静默必须来自规则开关，而不是通知门控。通知只在插入成功后
        // 判定，所以"没有新行"同时意味着没有通知判定。
        let db = Arc::new(Database::in_memory());
        let service = AlertService::new(Some(Arc::clone(&db)), None);
        {
            let mut settings = write_lock(&service.settings);
            settings.notify_failure = true;
            settings.notify_threshold = true;
            settings.alerts.rules.hostkey.enabled = false;
        }

        service.on_host_key_mismatch("srv_2", "SHA256:old -> SHA256:new");
        assert!(
            all_alerts(&db).is_empty(),
            "关闭的主机指纹规则不得插入任何行，连收口的行都不该出现"
        );

        // 已在告警中的行在关闭后要收口：再次触发只做 resolve。
        write_lock(&service.settings)
            .alerts
            .rules
            .hostkey
            .enabled = true;
        service.on_host_key_mismatch("srv_1", "SHA256:old -> SHA256:new");
        assert!(db.alert_active("srv_1", RULE_HOSTKEY).unwrap().is_some());

        write_lock(&service.settings)
            .alerts
            .rules
            .hostkey
            .enabled = false;
        service.on_host_key_mismatch("srv_1", "SHA256:old -> SHA256:newer");
        assert_eq!(db.alerts_active_count().unwrap(), 0, "关闭后收口已有告警");
        assert!(
            db.alert_active("srv_1", RULE_HOSTKEY).unwrap().is_none(),
            "被收口的行不得重新打开"
        );
    }

    #[test]
    fn per_server_hostkey_override_disables_only_that_server() {
        let db = Arc::new(Database::in_memory());
        let service = AlertService::new(Some(Arc::clone(&db)), None);
        write_lock(&service.settings)
            .alerts
            .per_server
            .entry("srv_1".to_string())
            .or_default()
            .hostkey = Some(crate::alerts::settings::ToggleRule { enabled: false });

        service.on_host_key_mismatch("srv_1", "SHA256:old -> SHA256:new");
        assert!(
            all_alerts(&db).is_empty(),
            "per-server 覆盖关闭后，这台服务器不该再写入主机指纹告警"
        );

        service.on_host_key_mismatch("srv_2", "SHA256:old -> SHA256:new");
        let row = db
            .alert_active("srv_2", RULE_HOSTKEY)
            .unwrap()
            .expect("没有覆盖的服务器仍按全局规则告警");
        assert_eq!(row.severity, "critical");
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

    #[test]
    fn should_notify_classifies_login_rules() {
        assert!(
            !should_notify(&notify_view(false, true), RULE_LOGIN, "warning"),
            "login failures follow the failure toggle, not the threshold toggle"
        );
        assert!(should_notify(&notify_view(true, false), RULE_LOGIN, "warning"));
        assert!(
            !should_notify(&notify_view(true, true), RULE_LOGIN_SOURCE, "info"),
            "new login sources are silent by design, even with every toggle on"
        );
    }

    fn login_report(failed: u32, accepted: &[&str]) -> LoginReport {
        LoginReport {
            available: true,
            source: "journal".to_string(),
            failed,
            accepted: accepted.iter().map(|ip| ip.to_string()).collect(),
        }
    }

    #[test]
    fn login_monitor_uses_first_report_as_baseline() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.ingest_login("srv_1", &login_report(9, &["203.0.113.9"]));
        assert_eq!(db.alerts_active_count().unwrap(), 0, "baseline is silent");
        service.ingest_login("srv_1", &login_report(9, &["203.0.113.9"]));
        let row = db.alert_active("srv_1", "login").unwrap().expect("login alert");
        assert_eq!(row.severity, "warning");
        service.ingest_login("srv_1", &login_report(1, &["203.0.113.9"]));
        assert_eq!(db.alerts_active_count().unwrap(), 0);
    }

    #[test]
    fn login_burst_is_announced_once_per_episode() {
        // 通知本身在测试构建里是空实现，所以这里固定"是否该弹"的判定：只有此前没有
        // 活跃行的上升沿才播报，`alert_insert` 的就地更新不会再次触发。
        let db = Arc::new(Database::in_memory());
        let first = db.alert_active("srv_1", RULE_LOGIN).unwrap();
        assert!(first.is_none());
        assert!(login_burst_is_new(first.as_ref()), "首次越线才是上升沿");

        db.alert_insert(&NewAlert {
            server_id: "srv_1".to_string(),
            server_name: "web-01".to_string(),
            rule_id: RULE_LOGIN.to_string(),
            severity: "warning".to_string(),
            title: "最近日志中失败登录 9 次".to_string(),
            detail: String::new(),
            first_seen_at: 1_000,
        })
        .unwrap();
        let active = db.alert_active("srv_1", RULE_LOGIN).unwrap();
        assert!(active.is_some());
        assert!(
            !login_burst_is_new(active.as_ref()),
            "同一波失败的后续轮次不得重复播报"
        );
    }

    #[test]
    fn repeated_failed_login_reports_refresh_one_alert() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.ingest_login("srv_1", &login_report(9, &["203.0.113.9"]));
        service.ingest_login("srv_1", &login_report(9, &["203.0.113.9"]));
        service.ingest_login("srv_1", &login_report(12, &["203.0.113.9"]));
        let rows = db
            .alerts_list(&AlertFilter {
                statuses: &["active"],
                limit: 50,
                offset: 0,
            })
            .unwrap();
        assert_eq!(rows.len(), 1, "one burst keeps exactly one active row");
        assert_eq!(rows[0].rule_id, RULE_LOGIN);
        assert!(
            rows[0].title.starts_with("最近日志中失败登录 12 次"),
            "title follows the newest count without claiming a time window, was {}",
            rows[0].title
        );
    }

    #[test]
    fn stopping_login_monitor_drops_its_status_entry() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.ingest_login("srv_1", &login_report(0, &[]));
        assert_eq!(service.list_alerts("open", 50, 0).unwrap().login_monitor.len(), 1);
        service.stop_login("srv_1");
        assert!(
            service.list_alerts("open", 50, 0).unwrap().login_monitor.is_empty(),
            "断开的服务器不该再出现在登录监控里"
        );
    }

    #[test]
    fn new_login_source_creates_info_alert() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.ingest_login("srv_1", &login_report(0, &["203.0.113.9"]));
        service.ingest_login(
            "srv_1",
            &login_report(0, &["203.0.113.9", "198.51.100.7"]),
        );
        let row = db
            .alert_active("srv_1", "login_source")
            .unwrap()
            .expect("source alert");
        assert_eq!(row.severity, "info");
        assert!(row.title.contains("198.51.100.7"));
    }

    #[test]
    fn unavailable_login_monitor_reports_reason_without_alerts() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.ingest_login(
            "srv_1",
            &LoginReport {
                available: false,
                source: String::new(),
                failed: 0,
                accepted: Vec::new(),
            },
        );
        assert_eq!(db.alerts_active_count().unwrap(), 0);
        let page = service.list_alerts("open", 50, 0).unwrap();
        assert_eq!(page.login_monitor.len(), 1);
        assert!(!page.login_monitor[0].available);
    }

    #[test]
    fn ignore_requires_reason_and_bounded_window() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.record_sample("srv_1", "web-01", &cpu(95.0));
        let id = db.alert_active("srv_1", "cpu").unwrap().unwrap().id;
        assert_eq!(
            service.ignore_alert(id, "   ", now_ms() + 60_000).unwrap_err().code,
            "INVALID_INPUT"
        );
        assert_eq!(
            service.ignore_alert(id, "维护中", now_ms() - 1).unwrap_err().code,
            "INVALID_INPUT"
        );
        assert_eq!(
            service
                .ignore_alert(id, "维护中", now_ms() + 8 * 24 * 60 * 60 * 1000)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
        service.ignore_alert(id, "维护中", now_ms() + 60_000).unwrap();
        assert!(db.alert_active("srv_1", "cpu").unwrap().is_none());
    }

    #[test]
    fn expired_ignore_reopens_as_a_new_active_row() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.record_sample("srv_1", "web-01", &cpu(95.0));
        let ignored_id = db.alert_active("srv_1", "cpu").unwrap().unwrap().id;
        db.alert_ignore(ignored_id, "维护窗口", 1).unwrap();

        service.record_sample("srv_1", "web-01", &cpu(97.0));

        let active = db
            .alert_active("srv_1", "cpu")
            .unwrap()
            .expect("忽略到期后条件仍成立，必须重新开启活跃行");
        assert_ne!(active.id, ignored_id, "重开是新行，历史行保持不动");
        assert_eq!(active.title, "CPU 使用率 97%");
        let rows = all_alerts(&db);
        assert_eq!(rows.len(), 2, "忽略行留在历史里，不参与重开");
        let ignored = rows.iter().find(|row| row.id == ignored_id).unwrap();
        assert_eq!(ignored.status, "ignored");
        assert_eq!(ignored.ignore_reason, "维护窗口");
    }

    #[test]
    fn unexpired_ignore_stays_closed() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.record_sample("srv_1", "web-01", &cpu(95.0));
        let id = db.alert_active("srv_1", "cpu").unwrap().unwrap().id;
        db.alert_ignore(id, "维护窗口", now_ms() + 60_000).unwrap();

        service.record_sample("srv_1", "web-01", &cpu(97.0));

        assert!(
            db.alert_active("srv_1", "cpu").unwrap().is_none(),
            "忽略窗口未到期不得重开"
        );
        assert_eq!(all_alerts(&db).len(), 1);
    }

    #[test]
    fn muted_rule_does_not_reopen_an_expired_ignore() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        service.record_sample("srv_1", "web-01", &cpu(95.0));
        let id = db.alert_active("srv_1", "cpu").unwrap().unwrap().id;
        db.alert_ignore(id, "维护窗口", 1).unwrap();
        db.alert_mute_add("srv_1", Some("cpu"), now_ms() + 60_000)
            .unwrap();

        service.record_sample("srv_1", "web-01", &cpu(97.0));

        assert!(
            db.alert_active("srv_1", "cpu").unwrap().is_none(),
            "静音期间与 Open 一致：即使忽略已到期也不重开"
        );
        assert_eq!(all_alerts(&db).len(), 1);
    }

    #[test]
    fn retention_prune_runs_at_most_once_an_hour() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        let stale = now_ms() - 40 * 24 * 60 * 60 * 1000;
        let mut event = NewAlert {
            server_id: "srv_1".to_string(),
            server_name: "web-01".to_string(),
            rule_id: "cpu".to_string(),
            severity: "warning".to_string(),
            title: "40 天前的告警".to_string(),
            detail: String::new(),
            first_seen_at: stale,
        };
        db.alert_insert(&event).unwrap();
        service.prune_if_due();
        assert!(
            db.alert_active("srv_1", "cpu").unwrap().is_none(),
            "首次补跑按保留窗口删除过期行"
        );

        event.title = "第二条过期告警".to_string();
        db.alert_insert(&event).unwrap();
        service.prune_if_due();
        assert!(
            db.alert_active("srv_1", "cpu").unwrap().is_some(),
            "限频窗口内不得再次裁剪"
        );
    }

    #[test]
    fn mute_validates_window_and_stores_rule() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        assert_eq!(
            service.mute_server("srv_1", None, now_ms() - 1).unwrap_err().code,
            "INVALID_INPUT"
        );
        service
            .mute_server("srv_1", Some("disk"), now_ms() + 60_000)
            .unwrap();
        let mutes = db.alert_mutes_for("srv_1").unwrap();
        assert_eq!(mutes.len(), 1);
        assert_eq!(mutes[0].rule_id.as_deref(), Some("disk"));
    }

    #[test]
    fn list_alerts_clamps_paging_and_filters_statuses() {
        let db = Arc::new(Database::in_memory());
        let service = service_with(&db);
        for index in 0..5 {
            db.alert_insert(&NewAlert {
                server_id: format!("srv_{index}"),
                server_name: format!("web-0{index}"),
                rule_id: "cpu".to_string(),
                severity: "warning".to_string(),
                title: format!("CPU {index}"),
                detail: String::new(),
                first_seen_at: 1_000 + index as i64,
            })
            .unwrap();
        }
        let page = service.list_alerts("open", 0, -10).unwrap();
        assert_eq!(page.items.len(), 1, "limit clamps to 1 and offset clamps to 0");
        assert_eq!(page.active_count, 5);
        assert!(page.login_monitor.is_empty(), "no monitor entries without a poll");

        let wide = service.list_alerts("open", 2_000, 0).unwrap();
        assert_eq!(wide.items.len(), 5, "limit clamps to 200, never rejects");
        assert_eq!(service.list_alerts("", 50, 0).unwrap().items.len(), 5);
        assert!(
            service.list_alerts("handled", 50, 0).unwrap().items.is_empty(),
            "no handled rows yet"
        );
    }
}
