use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, OptionalExtension, Transaction};
use serde::Serialize;

use super::{Database, DbError};

#[derive(Debug, Clone)]
pub struct NewAlert {
    pub server_id: String,
    pub server_name: String,
    pub rule_id: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub first_seen_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertRow {
    pub id: i64,
    pub server_id: String,
    pub server_name: String,
    pub rule_id: String,
    pub severity: String,
    pub status: String,
    pub title: String,
    pub detail: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub resolved_at: Option<i64>,
    pub ignore_reason: String,
    pub ignored_until: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AlertFilter {
    /// 空切片表示全部状态；`&["resolved", "ignored"]` 表示已处理。
    pub statuses: &'static [&'static str],
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone)]
pub struct MuteRow {
    pub rule_id: Option<String>,
    pub until_ms: i64,
}

const ROW_COLUMNS: &str = "a.id, a.server_id, \
    COALESCE(NULLIF(a.server_name, ''), s.name, a.server_id), \
    a.rule_id, a.severity, a.status, a.title, a.detail, a.first_seen_at, \
    a.last_seen_at, a.resolved_at, a.ignore_reason, a.ignored_until";

const ROW_FROM: &str = " FROM alerts a LEFT JOIN servers s ON s.id = a.server_id";

fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<AlertRow> {
    Ok(AlertRow {
        id: row.get(0)?,
        server_id: row.get(1)?,
        server_name: row.get(2)?,
        rule_id: row.get(3)?,
        severity: row.get(4)?,
        status: row.get(5)?,
        title: row.get(6)?,
        detail: row.get(7)?,
        first_seen_at: row.get(8)?,
        last_seen_at: row.get(9)?,
        resolved_at: row.get(10)?,
        ignore_reason: row.get(11)?,
        ignored_until: row.get(12)?,
    })
}

/// 活跃行就地更新、否则插入新行；调用方必须已在写事务里。
fn upsert_active_alert(tx: &Transaction<'_>, event: &NewAlert) -> Result<i64, DbError> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM alerts
             WHERE server_id = ?1 AND rule_id = ?2 AND status = 'active'",
            params![event.server_id, event.rule_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(DbError::write)?;
    if let Some(id) = existing {
        tx.execute(
            "UPDATE alerts SET title = ?2, detail = ?3, last_seen_at = ?4 WHERE id = ?1",
            params![id, event.title, event.detail, event.first_seen_at],
        )
        .map_err(DbError::write)?;
        return Ok(id);
    }
    tx.execute(
        "INSERT INTO alerts
           (server_id, server_name, rule_id, severity, status, title, detail,
            first_seen_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, ?7)",
        params![
            event.server_id,
            event.server_name,
            event.rule_id,
            event.severity,
            event.title,
            event.detail,
            event.first_seen_at
        ],
    )
    .map_err(DbError::write)?;
    Ok(tx.last_insert_rowid())
}

impl Database {
    pub(crate) fn alert_insert(&self, event: &NewAlert) -> Result<i64, DbError> {
        self.write_tx(|tx| upsert_active_alert(tx, event))
    }

    /// 忽略窗口到期后重新开启一条告警：`ignored` 行原样留在历史里，另插一行 `active`。
    /// 忽略期间引擎仍处 opened 态，只会发 `Touch`；没有这个入口就再也不会出现活跃行。
    /// 唯一部分索引要求此刻没有别的活跃行，所以存在活跃行时直接返回 false。
    pub(crate) fn alert_reopen_ignored(
        &self,
        server_id: &str,
        rule_id: &str,
        title: &str,
        now: i64,
    ) -> Result<bool, DbError> {
        self.write_tx(|tx| {
            let previous: Option<String> = tx
                .query_row(
                    "SELECT server_name FROM alerts
                     WHERE server_id = ?1 AND rule_id = ?2 AND status = 'ignored'
                       AND ignored_until IS NOT NULL AND ignored_until <= ?3
                     ORDER BY ignored_until DESC, id DESC LIMIT 1",
                    params![server_id, rule_id, now],
                    |row| row.get(0),
                )
                .optional()
                .map_err(DbError::write)?;
            let Some(server_name) = previous else {
                return Ok(false);
            };
            let active: Option<i64> = tx
                .query_row(
                    "SELECT id FROM alerts
                     WHERE server_id = ?1 AND rule_id = ?2 AND status = 'active'",
                    params![server_id, rule_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(DbError::write)?;
            if active.is_some() {
                return Ok(false);
            }
            upsert_active_alert(
                tx,
                &NewAlert {
                    server_id: server_id.to_string(),
                    server_name,
                    rule_id: rule_id.to_string(),
                    severity: crate::alerts::rules::severity(rule_id).to_string(),
                    title: title.to_string(),
                    detail: String::new(),
                    first_seen_at: now,
                },
            )?;
            Ok(true)
        })
    }

    pub(crate) fn alert_touch(
        &self,
        id: i64,
        title: &str,
        last_seen_at: i64,
    ) -> Result<(), DbError> {
        self.write_tx(|tx| {
            tx.execute(
                "UPDATE alerts SET title = ?2, last_seen_at = ?3 WHERE id = ?1",
                params![id, title, last_seen_at],
            )
            .map_err(DbError::write)?;
            Ok(())
        })
    }

    pub(crate) fn alert_resolve(&self, id: i64) -> Result<(), DbError> {
        self.write_tx(|tx| {
            tx.execute(
                "UPDATE alerts SET status = 'resolved', resolved_at = ?2
                 WHERE id = ?1 AND status = 'active'",
                params![id, super::now_ms()],
            )
            .map_err(DbError::write)?;
            Ok(())
        })
    }

    pub(crate) fn alert_ignore(
        &self,
        id: i64,
        reason: &str,
        until_ms: i64,
    ) -> Result<(), DbError> {
        self.write_tx(|tx| {
            tx.execute(
                "UPDATE alerts SET status = 'ignored', ignore_reason = ?2, ignored_until = ?3
                 WHERE id = ?1 AND status = 'active'",
                params![id, reason, until_ms],
            )
            .map_err(DbError::write)?;
            Ok(())
        })
    }

    pub(crate) fn alert_active(
        &self,
        server_id: &str,
        rule_id: &str,
    ) -> Result<Option<AlertRow>, DbError> {
        self.lock()?
            .query_row(
                &format!(
                    "SELECT {ROW_COLUMNS}{ROW_FROM}
                     WHERE a.server_id = ?1 AND a.rule_id = ?2 AND a.status = 'active'"
                ),
                params![server_id, rule_id],
                row_from,
            )
            .optional()
            .map_err(DbError::write)
    }

    pub(crate) fn alerts_list(&self, filter: &AlertFilter) -> Result<Vec<AlertRow>, DbError> {
        let conn = self.lock()?;
        let clause = if filter.statuses.is_empty() {
            String::new()
        } else {
            let marks = vec!["?"; filter.statuses.len()].join(", ");
            format!(" WHERE a.status IN ({marks})")
        };
        let mut statement = conn
            .prepare(&format!(
                "SELECT {ROW_COLUMNS}{ROW_FROM}{clause}
                 ORDER BY a.first_seen_at DESC LIMIT ? OFFSET ?"
            ))
            .map_err(DbError::write)?;
        let mut values: Vec<Value> = filter
            .statuses
            .iter()
            .map(|status| Value::Text((*status).to_string()))
            .collect();
        values.push(Value::Integer(filter.limit));
        values.push(Value::Integer(filter.offset));
        let rows = statement
            .query_map(params_from_iter(values.iter()), row_from)
            .map_err(DbError::write)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DbError::write)
    }

    pub(crate) fn alerts_active_count(&self) -> Result<i64, DbError> {
        self.lock()?
            .query_row(
                "SELECT count(*) FROM alerts WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
            .map_err(DbError::write)
    }

    pub(crate) fn alerts_resolve_server(&self, server_id: &str) -> Result<(), DbError> {
        self.write_tx(|tx| {
            tx.execute(
                "UPDATE alerts SET status = 'resolved', resolved_at = ?2
                 WHERE server_id = ?1 AND status = 'active'",
                params![server_id, super::now_ms()],
            )
            .map_err(DbError::write)?;
            Ok(())
        })
    }

    pub(crate) fn alerts_resolve_all_active(&self) -> Result<(), DbError> {
        self.write_tx(|tx| {
            tx.execute(
                "UPDATE alerts SET status = 'resolved', resolved_at = ?1
                 WHERE status = 'active'",
                params![super::now_ms()],
            )
            .map_err(DbError::write)?;
            Ok(())
        })
    }

    pub(crate) fn alert_mutes_for(&self, server_id: &str) -> Result<Vec<MuteRow>, DbError> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(
                "SELECT rule_id, until_ms FROM alert_mutes
                 WHERE server_id = ?1 AND until_ms > ?2
                 ORDER BY until_ms DESC",
            )
            .map_err(DbError::write)?;
        let rows = statement
            .query_map(params![server_id, super::now_ms()], |row| {
                Ok(MuteRow {
                    rule_id: row.get(0)?,
                    until_ms: row.get(1)?,
                })
            })
            .map_err(DbError::write)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DbError::write)
    }

    pub(crate) fn alert_mute_add(
        &self,
        server_id: &str,
        rule_id: Option<&str>,
        until_ms: i64,
    ) -> Result<(), DbError> {
        self.write_tx(|tx| {
            tx.execute(
                "DELETE FROM alert_mutes WHERE server_id = ?1 AND rule_id IS ?2",
                params![server_id, rule_id],
            )
            .map_err(DbError::write)?;
            tx.execute(
                "INSERT INTO alert_mutes (server_id, rule_id, until_ms, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![server_id, rule_id, until_ms, super::now_ms()],
            )
            .map_err(DbError::write)?;
            Ok(())
        })
    }

    pub(crate) fn login_source_touch(
        &self,
        server_id: &str,
        ip: &str,
        now_ms: i64,
    ) -> Result<bool, DbError> {
        self.write_tx(|tx| {
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT first_seen_at FROM login_sources
                     WHERE server_id = ?1 AND ip = ?2",
                    params![server_id, ip],
                    |row| row.get(0),
                )
                .optional()
                .map_err(DbError::write)?;
            match existing {
                Some(_) => {
                    tx.execute(
                        "UPDATE login_sources SET last_seen_at = ?3
                         WHERE server_id = ?1 AND ip = ?2",
                        params![server_id, ip, now_ms],
                    )
                    .map_err(DbError::write)?;
                    Ok(false)
                }
                None => {
                    tx.execute(
                        "INSERT INTO login_sources (server_id, ip, first_seen_at, last_seen_at)
                         VALUES (?1, ?2, ?3, ?3)",
                        params![server_id, ip, now_ms],
                    )
                    .map_err(DbError::write)?;
                    Ok(true)
                }
            }
        })
    }

    pub(crate) fn server_name(&self, server_id: &str) -> Result<String, DbError> {
        let name: Option<String> = self
            .lock()?
            .query_row("SELECT name FROM servers WHERE id = ?1", [server_id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(DbError::write)?;
        Ok(name.filter(|value| !value.is_empty()).unwrap_or_else(|| server_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn alert(rule_id: &str, at: i64) -> NewAlert {
        NewAlert {
            server_id: "srv_1".to_string(),
            server_name: "web-01".to_string(),
            rule_id: rule_id.to_string(),
            severity: "warning".to_string(),
            title: format!("{rule_id} 92%"),
            detail: String::new(),
            first_seen_at: at,
        }
    }

    #[test]
    fn active_rule_is_unique_and_touched() {
        let db = Database::in_memory();
        let first = db.alert_insert(&alert("cpu", 1_000)).unwrap();
        let again = db
            .alert_insert(&NewAlert {
                title: "CPU 使用率 95%".to_string(),
                first_seen_at: 2_000,
                ..alert("cpu", 2_000)
            })
            .unwrap();
        assert_eq!(first, again, "one active row per (server, rule)");
        let row = db.alert_active("srv_1", "cpu").unwrap().unwrap();
        assert_eq!(row.title, "CPU 使用率 95%");
        assert_eq!(row.last_seen_at, 2_000);

        db.alert_resolve(first).unwrap();
        assert!(db.alert_active("srv_1", "cpu").unwrap().is_none());
        let second = db.alert_insert(&alert("cpu", 3_000)).unwrap();
        assert_ne!(first, second, "a resolved rule can open a new row");
    }

    #[test]
    fn list_filters_and_counts() {
        let db = Database::in_memory();
        let a = db.alert_insert(&alert("cpu", 1_000)).unwrap();
        db.alert_insert(&alert("memory", 2_000)).unwrap();
        db.alert_resolve(a).unwrap();

        let active = db
            .alerts_list(&AlertFilter {
                statuses: &["active"],
                limit: 50,
                offset: 0,
            })
            .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].rule_id, "memory");
        assert_eq!(db.alerts_active_count().unwrap(), 1);

        let handled = db
            .alerts_list(&AlertFilter {
                statuses: &["resolved"],
                limit: 50,
                offset: 0,
            })
            .unwrap();
        assert_eq!(handled.len(), 1);
        assert_eq!(handled[0].rule_id, "cpu");

        db.alerts_resolve_server("srv_1").unwrap();
        assert_eq!(db.alerts_active_count().unwrap(), 0);
    }

    #[test]
    fn ignore_records_reason_and_deadline() {
        let db = Database::in_memory();
        let id = db.alert_insert(&alert("disk", 1_000)).unwrap();
        db.alert_ignore(id, "计划内扩容", 9_000).unwrap();
        let rows = db
            .alerts_list(&AlertFilter {
                statuses: &["ignored"],
                limit: 10,
                offset: 0,
            })
            .unwrap();
        assert_eq!(rows[0].ignore_reason, "计划内扩容");
        assert_eq!(rows[0].ignored_until, Some(9_000));
    }

    #[test]
    fn mutes_match_server_or_rule() {
        let db = Database::in_memory();
        // 仓储只返回未到期的静音，所以时间戳必须相对当前时刻。
        let now = crate::db::now_ms();
        db.alert_mute_add("srv_1", None, now + 5_000).unwrap();
        db.alert_mute_add("srv_1", Some("cpu"), now + 6_000).unwrap();
        db.alert_mute_add("srv_1", Some("expired"), now - 1).unwrap();
        let mutes = db.alert_mutes_for("srv_1").unwrap();
        assert_eq!(mutes.len(), 2);
        assert_eq!(mutes[0].rule_id.as_deref(), Some("cpu"), "newest deadline first");
        assert_eq!(mutes[0].until_ms, now + 6_000);
        assert!(db.alert_mutes_for("srv_2").unwrap().is_empty());
    }

    #[test]
    fn login_source_reports_first_occurrence_only() {
        let db = Database::in_memory();
        assert!(db.login_source_touch("srv_1", "203.0.113.9", 1_000).unwrap());
        assert!(!db.login_source_touch("srv_1", "203.0.113.9", 2_000).unwrap());
        assert!(db.login_source_touch("srv_1", "203.0.113.10", 3_000).unwrap());
    }

    #[test]
    fn server_name_falls_back_to_id() {
        let db = Database::in_memory();
        assert_eq!(db.server_name("missing").unwrap(), "missing");
    }
}
