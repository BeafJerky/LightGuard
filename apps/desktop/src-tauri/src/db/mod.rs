pub(crate) mod alert_store;
pub(crate) mod audit_store;
pub(crate) mod commands;
mod dto;

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;

pub use commands::{db_load, db_write};
pub(crate) use alert_store::{AlertFilter, AlertRow, MuteRow, NewAlert};
pub(crate) use audit_store::{AuditRow, NewAudit};
pub use dto::{
    DbOp, LoadResult, LogEntry, PersistedServer, StoredState, ThemeChoices, MAX_LOGS, MAX_SERVERS,
};

const DB_FILE: &str = "lightguard.db";
const SCHEMA_VERSION: i64 = 2;

const V1_SCHEMA: &str = r#"
BEGIN;
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS preferences (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS themes (
  scope TEXT PRIMARY KEY CHECK (scope IN ('console', 'desktop')),
  theme_id TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS servers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  hostname TEXT NOT NULL DEFAULT '',
  role TEXT NOT NULL DEFAULT '',
  host TEXT NOT NULL,
  port INTEGER NOT NULL,
  region TEXT NOT NULL DEFAULT '',
  group_name TEXT NOT NULL DEFAULT '',
  tags TEXT NOT NULL DEFAULT '[]',
  note TEXT NOT NULL DEFAULT '',
  user TEXT NOT NULL,
  auth TEXT NOT NULL,
  auth_label TEXT NOT NULL DEFAULT '',
  fingerprint TEXT NOT NULL DEFAULT '',
  last_sync TEXT NOT NULL DEFAULT '',
  last_seen TEXT NOT NULL DEFAULT '',
  cpu_cores INTEGER NOT NULL DEFAULT 0,
  mem_total REAL NOT NULL DEFAULT 0,
  disk_total REAL NOT NULL DEFAULT 0,
  os TEXT NOT NULL DEFAULT '',
  kernel TEXT NOT NULL DEFAULT '',
  arch TEXT NOT NULL DEFAULT '',
  cpu_model TEXT NOT NULL DEFAULT '',
  cpu_freq TEXT NOT NULL DEFAULT '',
  timezone TEXT NOT NULL DEFAULT '',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS logs (
  id TEXT PRIMARY KEY,
  ts INTEGER NOT NULL,
  server_id TEXT NOT NULL,
  server_name TEXT NOT NULL DEFAULT '',
  ip TEXT NOT NULL DEFAULT '',
  port INTEGER NOT NULL,
  action TEXT NOT NULL,
  status TEXT NOT NULL,
  duration TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT '',
  code TEXT NOT NULL DEFAULT '',
  auth TEXT NOT NULL DEFAULT '',
  client TEXT NOT NULL DEFAULT '',
  session TEXT NOT NULL DEFAULT '',
  fingerprint TEXT NOT NULL DEFAULT '',
  hint TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_logs_ts ON logs (ts DESC);
PRAGMA user_version = 1;
COMMIT;
"#;

const V2_SCHEMA: &str = r#"
BEGIN;
CREATE TABLE IF NOT EXISTS alerts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT NOT NULL,
  server_name TEXT NOT NULL DEFAULT '',
  rule_id TEXT NOT NULL,
  severity TEXT NOT NULL CHECK (severity IN ('critical', 'warning', 'info')),
  status TEXT NOT NULL CHECK (status IN ('active', 'resolved', 'ignored')),
  title TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  first_seen_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  resolved_at INTEGER,
  ignore_reason TEXT NOT NULL DEFAULT '',
  ignored_until INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_alerts_active_rule
  ON alerts (server_id, rule_id) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_alerts_status_time
  ON alerts (status, first_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_alerts_server_rule
  ON alerts (server_id, rule_id);

CREATE TABLE IF NOT EXISTS alert_mutes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT NOT NULL,
  rule_id TEXT,
  until_ms INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_alert_mutes_server ON alert_mutes (server_id, until_ms);

CREATE TABLE IF NOT EXISTS audit (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  at_ms INTEGER NOT NULL,
  server_id TEXT NOT NULL,
  server_name TEXT NOT NULL DEFAULT '',
  op TEXT NOT NULL,
  params TEXT NOT NULL DEFAULT '',
  result TEXT NOT NULL CHECK (result IN ('success', 'failed', 'unknown')),
  duration_ms INTEGER NOT NULL DEFAULT 0,
  permit_source TEXT NOT NULL DEFAULT '',
  rollback_of INTEGER
);
CREATE INDEX IF NOT EXISTS idx_audit_time ON audit (at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_audit_server ON audit (server_id, at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_audit_rollback ON audit (rollback_of);

CREATE TABLE IF NOT EXISTS login_sources (
  server_id TEXT NOT NULL,
  ip TEXT NOT NULL,
  first_seen_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  PRIMARY KEY (server_id, ip)
);
PRAGMA user_version = 2;
COMMIT;
"#;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbError {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl DbError {
    fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: hint.into(),
        }
    }

    pub fn unavailable(detail: impl std::fmt::Display) -> Self {
        Self::new(
            "DB_UNAVAILABLE",
            format!("本地数据库不可用：{detail}"),
            "请检查磁盘空间与文件权限；应用会暂时改用浏览器本地存储。",
        )
    }

    pub fn write(detail: impl std::fmt::Display) -> Self {
        Self::new(
            "DB_WRITE_FAILED",
            format!("写入本地数据库失败：{detail}"),
            "本次修改仍保留在内存中，请检查磁盘空间或重启应用。",
        )
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(
            "INVALID_INPUT",
            message,
            "请检查数据后重试。",
        )
    }
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for DbError {}

pub(crate) struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    fn open(config_dir: &Path) -> Result<Self, DbError> {
        std::fs::create_dir_all(config_dir).map_err(DbError::unavailable)?;
        let conn = Connection::open(config_dir.join(DB_FILE)).map_err(DbError::unavailable)?;
        configure(&conn)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, DbError> {
        self.conn
            .lock()
            .map_err(|_| DbError::write("数据库锁已损坏，请重启应用"))
    }

    fn write_tx<T>(
        &self,
        task: impl FnOnce(&Transaction<'_>) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(DbError::write)?;
        let value = task(&tx)?;
        tx.commit().map_err(DbError::write)?;
        Ok(value)
    }

    fn load(&self) -> Result<LoadResult, DbError> {
        let conn = self.lock()?;
        Ok(LoadResult {
            initialized: get_meta(&conn, "initialized")?.as_deref() == Some("1"),
            themes: load_themes(&conn)?,
            settings: load_pref(&conn, "app.settings")?,
            servers: load_servers(&conn)?,
            logs: load_logs(&conn)?,
            backup: load_pref(&conn, "backup.summary")?,
            render: load_pref(&conn, "render.settings")?,
        })
    }

    fn apply(&self, op: DbOp) -> Result<(), DbError> {
        match op {
            DbOp::Replace { snapshot } => self.replace(snapshot),
            DbOp::Reset => self.reset(),
            DbOp::ServerUpsert { server } => {
                dto::validate_server(&server)?;
                self.write_tx(|tx| upsert_server(tx, &server, None))
            }
            DbOp::ServerDelete { id } => {
                if id.is_empty() {
                    return Err(DbError::invalid_input("服务器标识无效"));
                }
                self.write_tx(|tx| {
                    tx.execute("DELETE FROM servers WHERE id = ?1", [&id])
                        .map_err(DbError::write)?;
                    Ok(())
                })
            }
            DbOp::LogAppend { log } => {
                dto::validate_log(&log)?;
                self.write_tx(|tx| {
                    insert_log(tx, &log)?;
                    trim_logs(tx)
                })
            }
            DbOp::LogsClear => self.write_tx(|tx| {
                tx.execute("DELETE FROM logs", [])
                    .map_err(DbError::write)?;
                Ok(())
            }),
            DbOp::ThemesSet { themes } => {
                dto::validate_themes(&themes)?;
                self.write_tx(|tx| save_themes(tx, &themes))
            }
            DbOp::SettingsSet { settings } => {
                dto::validate_settings(&settings)?;
                self.write_tx(|tx| save_pref(tx, "app.settings", &settings))
            }
            DbOp::BackupSet { summary } => {
                if let Some(value) = &summary {
                    dto::validate_backup(value)?;
                }
                self.write_tx(|tx| match &summary {
                    Some(value) => save_pref(tx, "backup.summary", value),
                    None => delete_pref(tx, "backup.summary"),
                })
            }
            DbOp::RenderSet { settings } => {
                dto::validate_render(&settings)?;
                self.write_tx(|tx| save_pref(tx, "render.settings", &settings))
            }
        }
    }

    fn replace(&self, snapshot: StoredState) -> Result<(), DbError> {
        if snapshot.servers.len() > MAX_SERVERS {
            return Err(DbError::invalid_input(format!(
                "服务器数量超过 {MAX_SERVERS} 台限制"
            )));
        }
        if snapshot.logs.len() > MAX_LOGS * 4 {
            return Err(DbError::invalid_input("连接记录数量超出限制"));
        }
        for server in &snapshot.servers {
            dto::validate_server(server)?;
        }
        for log in &snapshot.logs {
            dto::validate_log(log)?;
        }
        if let Some(themes) = &snapshot.themes {
            dto::validate_themes(themes)?;
        }
        if let Some(settings) = &snapshot.settings {
            dto::validate_settings(settings)?;
        }
        if let Some(backup) = &snapshot.backup {
            dto::validate_backup(backup)?;
        }
        if let Some(render) = &snapshot.render {
            dto::validate_render(render)?;
        }

        self.write_tx(|tx| {
            tx.execute("DELETE FROM servers", [])
                .map_err(DbError::write)?;
            for (index, server) in snapshot.servers.iter().enumerate() {
                upsert_server(tx, server, Some(index as i64))?;
            }

            tx.execute("DELETE FROM logs", []).map_err(DbError::write)?;
            for log in snapshot.logs.iter().take(MAX_LOGS) {
                insert_log(tx, log)?;
            }

            tx.execute("DELETE FROM themes", []).map_err(DbError::write)?;
            if let Some(themes) = &snapshot.themes {
                save_themes(tx, themes)?;
            }

            match &snapshot.settings {
                Some(settings) => save_pref(tx, "app.settings", settings)?,
                None => delete_pref(tx, "app.settings")?,
            }
            match &snapshot.backup {
                Some(backup) => save_pref(tx, "backup.summary", backup)?,
                None => delete_pref(tx, "backup.summary")?,
            }
            // 状态快照由 store 维护，渲染偏好单独持久化；快照未携带时保持原值。
            if let Some(render) = &snapshot.render {
                save_pref(tx, "render.settings", render)?;
            }
            set_meta(tx, "initialized", "1")
        })
    }

    fn reset(&self) -> Result<(), DbError> {
        self.write_tx(|tx| {
            tx.execute("DELETE FROM servers", [])
                .map_err(DbError::write)?;
            tx.execute("DELETE FROM logs", []).map_err(DbError::write)?;
            tx.execute("DELETE FROM themes", []).map_err(DbError::write)?;
            tx.execute("DELETE FROM preferences", [])
                .map_err(DbError::write)?;
            tx.execute("DELETE FROM alerts", [])
                .map_err(DbError::write)?;
            tx.execute("DELETE FROM alert_mutes", [])
                .map_err(DbError::write)?;
            tx.execute("DELETE FROM audit", [])
                .map_err(DbError::write)?;
            tx.execute("DELETE FROM login_sources", [])
                .map_err(DbError::write)?;
            set_meta(tx, "initialized", "1")
        })
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        configure(&conn).expect("configure sqlite");
        migrate(&conn).expect("migrate sqlite");
        Self {
            conn: Mutex::new(conn),
        }
    }
}

pub struct DbState {
    database: Option<Arc<Database>>,
    error: Option<String>,
}

impl DbState {
    pub fn new(config_dir: &Path) -> Self {
        match Database::open(config_dir) {
            Ok(database) => Self {
                database: Some(Arc::new(database)),
                error: None,
            },
            Err(error) => {
                eprintln!("[lightguard] database unavailable: {error}");
                Self {
                    database: None,
                    error: Some(error.to_string()),
                }
            }
        }
    }

    pub(crate) fn handle(&self) -> Option<Arc<Database>> {
        self.database.clone()
    }

    fn database(&self) -> Result<Arc<Database>, DbError> {
        self.database
            .clone()
            .ok_or_else(|| DbError::unavailable(self.error.clone().unwrap_or_default()))
    }
}

fn configure(conn: &Connection) -> Result<(), DbError> {
    conn.busy_timeout(Duration::from_secs(3))
        .map_err(DbError::unavailable)?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(DbError::unavailable)?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(DbError::unavailable)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(DbError::unavailable)?;
    Ok(())
}

fn migrate(conn: &Connection) -> Result<(), DbError> {
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(DbError::unavailable)?;
    if version < 1 {
        conn.execute_batch(V1_SCHEMA)
            .map_err(DbError::unavailable)?;
    }
    if version < SCHEMA_VERSION {
        conn.execute_batch(V2_SCHEMA)
            .map_err(DbError::unavailable)?;
    }
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0)
}

fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>, DbError> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .optional()
    .map_err(DbError::write)
}

fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(DbError::write)?;
    Ok(())
}

fn load_pref(conn: &Connection, key: &str) -> Result<Option<serde_json::Value>, DbError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM preferences WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(DbError::write)?;
    Ok(raw.and_then(|text| serde_json::from_str(&text).ok()))
}

fn save_pref(
    conn: &Connection,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), DbError> {
    let raw = serde_json::to_string(value).map_err(DbError::write)?;
    conn.execute(
        "INSERT INTO preferences (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, raw, now_ms()],
    )
    .map_err(DbError::write)?;
    Ok(())
}

fn delete_pref(conn: &Connection, key: &str) -> Result<(), DbError> {
    conn.execute("DELETE FROM preferences WHERE key = ?1", [key])
        .map_err(DbError::write)?;
    Ok(())
}

fn save_themes(conn: &Connection, themes: &ThemeChoices) -> Result<(), DbError> {
    let now = now_ms();
    for (scope, theme) in [("console", &themes.console), ("desktop", &themes.desktop)] {
        conn.execute(
            "INSERT INTO themes (scope, theme_id, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(scope) DO UPDATE SET theme_id = excluded.theme_id, updated_at = excluded.updated_at",
            params![scope, theme, now],
        )
        .map_err(DbError::write)?;
    }
    Ok(())
}

fn load_themes(conn: &Connection) -> Result<Option<ThemeChoices>, DbError> {
    let mut statement = conn
        .prepare("SELECT scope, theme_id FROM themes")
        .map_err(DbError::write)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(DbError::write)?;
    let mut console = None;
    let mut desktop = None;
    for row in rows {
        let (scope, theme) = row.map_err(DbError::write)?;
        match scope.as_str() {
            "console" => console = Some(theme),
            "desktop" => desktop = Some(theme),
            _ => {}
        }
    }
    Ok(match (console, desktop) {
        (Some(console), Some(desktop)) => Some(ThemeChoices { console, desktop }),
        _ => None,
    })
}

fn upsert_server(
    conn: &Connection,
    server: &PersistedServer,
    sort_order: Option<i64>,
) -> Result<(), DbError> {
    let tags = serde_json::to_string(&server.tags).map_err(DbError::write)?;
    let sort_order = match sort_order {
        Some(value) => value,
        None => conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM servers",
                [],
                |row| row.get(0),
            )
            .map_err(DbError::write)?,
    };
    let now = now_ms();
    conn.execute(
        "INSERT INTO servers (
           id, name, hostname, role, host, port, region, group_name, tags, note, user,
           auth, auth_label, fingerprint, last_sync, last_seen, cpu_cores, mem_total,
           disk_total, os, kernel, arch, cpu_model, cpu_freq, timezone, sort_order,
           created_at, updated_at
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
           ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28
         )
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           hostname = excluded.hostname,
           role = excluded.role,
           host = excluded.host,
           port = excluded.port,
           region = excluded.region,
           group_name = excluded.group_name,
           tags = excluded.tags,
           note = excluded.note,
           user = excluded.user,
           auth = excluded.auth,
           auth_label = excluded.auth_label,
           fingerprint = excluded.fingerprint,
           last_sync = excluded.last_sync,
           last_seen = excluded.last_seen,
           cpu_cores = excluded.cpu_cores,
           mem_total = excluded.mem_total,
           disk_total = excluded.disk_total,
           os = excluded.os,
           kernel = excluded.kernel,
           arch = excluded.arch,
           cpu_model = excluded.cpu_model,
           cpu_freq = excluded.cpu_freq,
           timezone = excluded.timezone,
           updated_at = excluded.updated_at",
        params![
            server.id,
            server.name,
            server.hostname,
            server.role,
            server.ip,
            server.port,
            server.region,
            server.group,
            tags,
            server.note,
            server.user,
            server.auth,
            server.auth_label,
            server.fingerprint,
            server.last_sync,
            server.last_seen,
            server.cpu_cores,
            server.mem_total,
            server.disk_total,
            server.os,
            server.kernel,
            server.arch,
            server.cpu_model,
            server.cpu_freq,
            server.timezone,
            sort_order,
            now,
            now,
        ],
    )
    .map_err(DbError::write)?;
    Ok(())
}

fn load_servers(conn: &Connection) -> Result<Vec<PersistedServer>, DbError> {
    let mut statement = conn
        .prepare(
            "SELECT id, name, hostname, role, host, port, region, group_name, tags, note,
                    user, auth, auth_label, fingerprint, last_sync, last_seen, cpu_cores,
                    mem_total, disk_total, os, kernel, arch, cpu_model, cpu_freq, timezone
             FROM servers ORDER BY sort_order ASC, created_at ASC",
        )
        .map_err(DbError::write)?;
    let rows = statement
        .query_map([], |row| {
            let tags: String = row.get(8)?;
            Ok(PersistedServer {
                id: row.get(0)?,
                name: row.get(1)?,
                hostname: row.get(2)?,
                role: row.get(3)?,
                ip: row.get(4)?,
                port: row.get(5)?,
                region: row.get(6)?,
                group: row.get(7)?,
                tags: serde_json::from_str(&tags).unwrap_or_default(),
                note: row.get(9)?,
                user: row.get(10)?,
                auth: row.get(11)?,
                auth_label: row.get(12)?,
                fingerprint: row.get(13)?,
                last_sync: row.get(14)?,
                last_seen: row.get(15)?,
                cpu_cores: row.get(16)?,
                mem_total: row.get(17)?,
                disk_total: row.get(18)?,
                os: row.get(19)?,
                kernel: row.get(20)?,
                arch: row.get(21)?,
                cpu_model: row.get(22)?,
                cpu_freq: row.get(23)?,
                timezone: row.get(24)?,
            })
        })
        .map_err(DbError::write)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(DbError::write)
}

fn insert_log(conn: &Connection, log: &LogEntry) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR REPLACE INTO logs (
           id, ts, server_id, server_name, ip, port, action, status, duration,
           source, code, auth, client, session, fingerprint, hint
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
         )",
        params![
            log.id,
            log.ts,
            log.server_id,
            log.server_name,
            log.ip,
            log.port,
            log.action,
            log.status,
            log.duration,
            log.source,
            log.code,
            log.auth,
            log.client,
            log.session,
            log.fingerprint,
            log.hint,
        ],
    )
    .map_err(DbError::write)?;
    Ok(())
}

fn trim_logs(conn: &Connection) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM logs WHERE rowid NOT IN (
           SELECT rowid FROM logs ORDER BY ts DESC, rowid DESC LIMIT ?1
         )",
        [MAX_LOGS as i64],
    )
    .map_err(DbError::write)?;
    Ok(())
}

fn load_logs(conn: &Connection) -> Result<Vec<LogEntry>, DbError> {
    let mut statement = conn
        .prepare(
            "SELECT id, ts, server_id, server_name, ip, port, action, status, duration,
                    source, code, auth, client, session, fingerprint, hint
             FROM logs ORDER BY ts DESC, rowid DESC LIMIT ?1",
        )
        .map_err(DbError::write)?;
    let rows = statement
        .query_map([MAX_LOGS as i64], |row| {
            Ok(LogEntry {
                id: row.get(0)?,
                ts: row.get(1)?,
                server_id: row.get(2)?,
                server_name: row.get(3)?,
                ip: row.get(4)?,
                port: row.get(5)?,
                action: row.get(6)?,
                status: row.get(7)?,
                duration: row.get(8)?,
                source: row.get(9)?,
                code: row.get(10)?,
                auth: row.get(11)?,
                client: row.get(12)?,
                session: row.get(13)?,
                fingerprint: row.get(14)?,
                hint: row.get(15)?,
            })
        })
        .map_err(DbError::write)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(DbError::write)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_server(id: &str) -> PersistedServer {
        PersistedServer {
            id: id.to_string(),
            name: "生产 Web".to_string(),
            hostname: "web-prod-01".to_string(),
            role: "Web".to_string(),
            ip: "192.168.5.4".to_string(),
            port: 22,
            region: "SSH 直连".to_string(),
            group: "生产".to_string(),
            tags: vec!["web".to_string()],
            note: "只读采集".to_string(),
            user: "admin".to_string(),
            auth: "password".to_string(),
            auth_label: "密码".to_string(),
            fingerprint: "SHA256:abc".to_string(),
            last_sync: "刚刚".to_string(),
            last_seen: "刚刚".to_string(),
            cpu_cores: 4,
            mem_total: 16.0,
            disk_total: 240.0,
            os: "Debian 12".to_string(),
            kernel: "6.1.0".to_string(),
            arch: "x86_64".to_string(),
            cpu_model: "Xeon".to_string(),
            cpu_freq: "2.4 GHz".to_string(),
            timezone: "CST".to_string(),
        }
    }

    fn sample_log(id: &str, ts: i64) -> LogEntry {
        LogEntry {
            id: id.to_string(),
            ts,
            server_id: "srv_1".to_string(),
            server_name: "生产 Web".to_string(),
            ip: "192.168.5.4".to_string(),
            port: 22,
            action: "连接成功".to_string(),
            status: "success".to_string(),
            duration: "0.42s".to_string(),
            source: "桌面客户端".to_string(),
            code: "OK".to_string(),
            auth: "密码".to_string(),
            client: "LightGuard Desktop".to_string(),
            session: "sess_1".to_string(),
            fingerprint: "SHA256:abc".to_string(),
            hint: String::new(),
        }
    }

    #[test]
    fn fresh_database_is_not_initialized() {
        let db = Database::in_memory();
        let state = db.load().unwrap();
        assert!(!state.initialized);
        assert!(state.themes.is_none());
        assert!(state.settings.is_none());
        assert!(state.servers.is_empty());
        assert!(state.logs.is_empty());
    }

    #[test]
    fn replace_round_trips_all_persisted_state() {
        let db = Database::in_memory();
        db.apply(DbOp::Replace {
            snapshot: StoredState {
                settings: Some(json!({ "interval": 5, "autoRefresh": true })),
                themes: Some(ThemeChoices {
                    console: "dark".to_string(),
                    desktop: "pixel".to_string(),
                }),
                servers: vec![sample_server("srv_1")],
                logs: vec![sample_log("log-1", 1000)],
                backup: Some(json!({ "at": "2026-10-28T00:00:00Z", "hosts": 1, "encrypted": true })),
                render: Some(json!({ "global": "auto" })),
            },
        })
        .unwrap();

        let state = db.load().unwrap();
        assert!(state.initialized);
        assert_eq!(state.servers.len(), 1);
        assert_eq!(state.servers[0].ip, "192.168.5.4");
        assert_eq!(state.servers[0].group, "生产");
        assert_eq!(state.servers[0].tags, vec!["web"]);
        assert_eq!(state.logs.len(), 1);
        assert_eq!(state.themes.unwrap().desktop, "pixel");
        assert_eq!(state.settings.unwrap()["interval"], 5);
        assert_eq!(state.backup.unwrap()["hosts"], 1);
        assert_eq!(state.render.unwrap()["global"], "auto");
    }

    #[test]
    fn server_upsert_keeps_order_and_delete_removes_row() {
        let db = Database::in_memory();
        db.apply(DbOp::ServerUpsert {
            server: sample_server("srv_1"),
        })
        .unwrap();
        db.apply(DbOp::ServerUpsert {
            server: sample_server("srv_2"),
        })
        .unwrap();

        let mut renamed = sample_server("srv_1");
        renamed.name = "生产 API".to_string();
        renamed.group = "预发".to_string();
        db.apply(DbOp::ServerUpsert { server: renamed })
            .unwrap();

        let state = db.load().unwrap();
        assert_eq!(state.servers.len(), 2);
        assert_eq!(state.servers[0].id, "srv_1");
        assert_eq!(state.servers[0].name, "生产 API");
        assert_eq!(state.servers[1].id, "srv_2");

        db.apply(DbOp::ServerDelete {
            id: "srv_1".to_string(),
        })
        .unwrap();
        let state = db.load().unwrap();
        assert_eq!(state.servers.len(), 1);
        assert_eq!(state.servers[0].id, "srv_2");
    }

    #[test]
    fn logs_keep_only_the_latest_window() {
        let db = Database::in_memory();
        for index in 0..(MAX_LOGS + 12) {
            db.apply(DbOp::LogAppend {
                log: sample_log(&format!("log-{index}"), index as i64),
            })
            .unwrap();
        }
        let state = db.load().unwrap();
        assert_eq!(state.logs.len(), MAX_LOGS);
        assert_eq!(state.logs[0].id, format!("log-{}", MAX_LOGS + 11));

        db.apply(DbOp::LogsClear).unwrap();
        assert!(db.load().unwrap().logs.is_empty());
    }

    #[test]
    fn reset_clears_content_but_stays_initialized() {
        let db = Database::in_memory();
        db.apply(DbOp::Replace {
            snapshot: StoredState {
                settings: Some(json!({ "interval": 5 })),
                themes: Some(ThemeChoices {
                    console: "nes".to_string(),
                    desktop: "tech".to_string(),
                }),
                servers: vec![sample_server("srv_1")],
                logs: vec![sample_log("log-1", 1000)],
                backup: None,
                render: None,
            },
        })
        .unwrap();
        db.apply(DbOp::Reset).unwrap();

        let state = db.load().unwrap();
        assert!(state.initialized);
        assert!(state.servers.is_empty());
        assert!(state.logs.is_empty());
        assert!(state.themes.is_none());
        assert!(state.settings.is_none());
    }

    #[test]
    fn rejects_invalid_server_input() {
        let db = Database::in_memory();
        let mut server = sample_server("srv_1");
        server.port = 0;
        assert!(db.apply(DbOp::ServerUpsert { server }).is_err());

        let mut server = sample_server("srv_2");
        server.ip = "   ".to_string();
        assert!(db.apply(DbOp::ServerUpsert { server }).is_err());
    }

    #[test]
    fn settings_and_render_survive_replacement() {
        let db = Database::in_memory();
        db.apply(DbOp::SettingsSet {
            settings: json!({ "interval": 10, "autoRefresh": false }),
        })
        .unwrap();
        db.apply(DbOp::RenderSet {
            settings: json!({ "global": "dom", "tiers": { "terminal": "canvas" } }),
        })
        .unwrap();

        let state = db.load().unwrap();
        assert_eq!(state.settings.unwrap()["interval"], 10);
        assert_eq!(state.render.unwrap()["tiers"]["terminal"], "canvas");
    }

    #[test]
    fn on_disk_database_reopens_with_same_data() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = Database::open(dir.path()).unwrap();
            db.apply(DbOp::Replace {
                snapshot: StoredState {
                    settings: Some(json!({ "interval": 7 })),
                    themes: Some(ThemeChoices {
                        console: "claude".to_string(),
                        desktop: "nes".to_string(),
                    }),
                    servers: vec![sample_server("srv_1")],
                    logs: vec![],
                    backup: None,
                    render: None,
                },
            })
            .unwrap();
        }
        let db = Database::open(dir.path()).unwrap();
        let state = db.load().unwrap();
        assert!(state.initialized);
        assert_eq!(state.servers.len(), 1);
        assert_eq!(state.themes.unwrap().console, "claude");
    }

    #[test]
    fn v1_database_migrates_to_v2_and_keeps_rows() {
        let conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        conn.execute_batch(V1_SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO preferences (key, value, updated_at) VALUES ('app.settings', '{\"interval\":5}', 1)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        for table in ["alerts", "alert_mutes", "audit", "login_sources"] {
            let count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "{table} must exist");
        }
        let value: String = conn
            .query_row(
                "SELECT value FROM preferences WHERE key = 'app.settings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(value.contains("interval"));
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn reset_clears_alert_tables() {
        let db = Database::in_memory();
        db.write_tx(|tx| {
            tx.execute(
                "INSERT INTO alerts (server_id, rule_id, severity, status, title, first_seen_at, last_seen_at)
                 VALUES ('srv', 'cpu', 'warning', 'active', 'CPU', 1, 1)",
                [],
            )
            .map_err(DbError::write)?;
            Ok(())
        })
        .unwrap();

        db.reset().unwrap();

        let count: i64 = db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM alerts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
