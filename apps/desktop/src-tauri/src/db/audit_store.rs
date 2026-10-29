use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{Database, DbError};

pub(crate) const RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;
pub(crate) const RETENTION_ROWS: i64 = 500;
const LOGIN_SOURCE_RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct NewAudit {
    pub at_ms: i64,
    pub server_id: String,
    pub server_name: String,
    pub op: String,
    pub params: String,
    pub result: String,
    pub duration_ms: i64,
    pub permit_source: String,
    pub rollback_of: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRow {
    pub id: i64,
    pub at_ms: i64,
    pub server_id: String,
    pub server_name: String,
    pub op: String,
    pub params: String,
    pub result: String,
    pub duration_ms: i64,
    pub permit_source: String,
    pub rollback_of: Option<i64>,
}

const AUDIT_COLUMNS: &str = "id, at_ms, server_id, server_name, op, params, result, \
    duration_ms, permit_source, rollback_of";

fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditRow> {
    Ok(AuditRow {
        id: row.get(0)?,
        at_ms: row.get(1)?,
        server_id: row.get(2)?,
        server_name: row.get(3)?,
        op: row.get(4)?,
        params: row.get(5)?,
        result: row.get(6)?,
        duration_ms: row.get(7)?,
        permit_source: row.get(8)?,
        rollback_of: row.get(9)?,
    })
}

impl Database {
    pub(crate) fn audit_insert(&self, entry: &NewAudit) -> Result<i64, DbError> {
        self.write_tx(|tx| {
            tx.execute(
                "INSERT INTO audit
                   (at_ms, server_id, server_name, op, params, result, duration_ms,
                    permit_source, rollback_of)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entry.at_ms,
                    entry.server_id,
                    entry.server_name,
                    entry.op,
                    entry.params,
                    entry.result,
                    entry.duration_ms,
                    entry.permit_source,
                    entry.rollback_of
                ],
            )
            .map_err(DbError::write)?;
            Ok(tx.last_insert_rowid())
        })
    }

    pub(crate) fn audit_list(&self, limit: i64, offset: i64) -> Result<Vec<AuditRow>, DbError> {
        let conn = self.lock()?;
        let mut statement = conn
            .prepare(&format!(
                "SELECT {AUDIT_COLUMNS} FROM audit ORDER BY at_ms DESC, id DESC LIMIT ?1 OFFSET ?2"
            ))
            .map_err(DbError::write)?;
        let rows = statement
            .query_map(params![limit, offset], row_from)
            .map_err(DbError::write)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DbError::write)
    }

    pub(crate) fn audit_get(&self, id: i64) -> Result<Option<AuditRow>, DbError> {
        self.lock()?
            .query_row(
                &format!("SELECT {AUDIT_COLUMNS} FROM audit WHERE id = ?1"),
                [id],
                row_from,
            )
            .optional()
            .map_err(DbError::write)
    }

    pub(crate) fn audit_has_rollback(&self, id: i64) -> Result<bool, DbError> {
        // 只有成功的回退才算"已回退"：一次失败的 `rmdir`（ENOTEMPTY、权限不足）
        // 不能把原始记录永久标成不可回退。
        let count: i64 = self
            .lock()?
            .query_row(
                "SELECT count(*) FROM audit WHERE rollback_of = ?1 AND result = 'success'",
                [id],
                |row| row.get(0),
            )
            .map_err(DbError::write)?;
        Ok(count > 0)
    }

    pub(crate) fn prune_retention(&self, now_ms: i64) -> Result<(), DbError> {
        let cutoff = now_ms - RETENTION_MS;
        self.write_tx(|tx| {
            tx.execute("DELETE FROM alerts WHERE first_seen_at < ?1", [cutoff])
                .map_err(DbError::write)?;
            tx.execute(
                "DELETE FROM alerts WHERE id NOT IN
                   (SELECT id FROM alerts ORDER BY first_seen_at DESC LIMIT ?1)",
                [RETENTION_ROWS],
            )
            .map_err(DbError::write)?;
            tx.execute("DELETE FROM audit WHERE at_ms < ?1", [cutoff])
                .map_err(DbError::write)?;
            tx.execute(
                "DELETE FROM audit WHERE id NOT IN
                   (SELECT id FROM audit ORDER BY at_ms DESC, id DESC LIMIT ?1)",
                [RETENTION_ROWS],
            )
            .map_err(DbError::write)?;
            tx.execute("DELETE FROM alert_mutes WHERE until_ms < ?1", [now_ms])
                .map_err(DbError::write)?;
            tx.execute(
                "DELETE FROM login_sources WHERE last_seen_at < ?1",
                [now_ms - LOGIN_SOURCE_RETENTION_MS],
            )
            .map_err(DbError::write)?;
            Ok(())
        })
    }
}

/// 测试专用：删掉 `audit` 表，让后续 `audit_insert` 稳定失败。只在测试构建里
/// 编译，不进入生产 API；`guard::write` 用它在不触碰会话的前提下验证"审计写不
/// 进去"时错误不会被吞掉。
#[cfg(test)]
impl Database {
    pub(crate) fn drop_audit_table_for_test(&self) {
        self.lock()
            .expect("lock sqlite")
            .execute_batch("DROP TABLE audit")
            .expect("drop audit table");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: i64) -> NewAudit {
        NewAudit {
            at_ms: at,
            server_id: "srv_1".to_string(),
            server_name: "web-01".to_string(),
            op: "fs.mkdir".to_string(),
            params: r#"{"path":"/vol1/1000/lg-filetest/lg-guard-test"}"#.to_string(),
            result: "success".to_string(),
            duration_ms: 12,
            permit_source: "unlock:15m".to_string(),
            rollback_of: None,
        }
    }

    #[test]
    fn audit_is_append_only_and_readable() {
        let db = Database::in_memory();
        let id = db.audit_insert(&entry(1_000)).unwrap();
        let rows = db.audit_list(10, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].op, "fs.mkdir");
        assert_eq!(rows[0].permit_source, "unlock:15m");
        assert!(db.audit_get(id).unwrap().is_some());
        assert!(!db.audit_has_rollback(id).unwrap());

        db.audit_insert(&NewAudit {
            op: "fs.mkdir.rollback".to_string(),
            result: "success".to_string(),
            rollback_of: Some(id),
            ..entry(2_000)
        })
        .unwrap();
        assert!(db.audit_has_rollback(id).unwrap());
        assert_eq!(db.audit_list(10, 0).unwrap().len(), 2);
    }

    #[test]
    fn only_successful_rollbacks_block_another_attempt() {
        let db = Database::in_memory();
        let original = db.audit_insert(&entry(1_000)).unwrap();
        assert!(!db.audit_has_rollback(original).unwrap());

        // 一次失败的 `rmdir`（ENOTEMPTY、权限不足）不构成"已回退"。
        db.audit_insert(&NewAudit {
            op: "fs.mkdir.rollback".to_string(),
            result: "failed".to_string(),
            rollback_of: Some(original),
            ..entry(2_000)
        })
        .unwrap();
        assert!(
            !db.audit_has_rollback(original).unwrap(),
            "failed 回退不能永久挡住重试"
        );

        // 结果未知的回退同样不能算成功。
        db.audit_insert(&NewAudit {
            op: "fs.mkdir.rollback".to_string(),
            result: "unknown".to_string(),
            rollback_of: Some(original),
            ..entry(3_000)
        })
        .unwrap();
        assert!(!db.audit_has_rollback(original).unwrap());

        db.audit_insert(&NewAudit {
            op: "fs.mkdir.rollback".to_string(),
            result: "success".to_string(),
            rollback_of: Some(original),
            ..entry(4_000)
        })
        .unwrap();
        assert!(db.audit_has_rollback(original).unwrap());
    }

    #[test]
    fn retention_trims_rows_age_and_expired_aux_tables() {
        let db = Database::in_memory();
        let now = LOGIN_SOURCE_RETENTION_MS * 3;
        // 年龄裁剪会删掉 30 天前的行，所以有效样本必须落在保留窗口内。
        let expired = db.audit_insert(&entry(now - RETENTION_MS - 1)).unwrap();
        for index in 0..(RETENTION_ROWS - 1) {
            db.audit_insert(&entry(now - 30_000 + index)).unwrap();
        }
        let stale_alert_at = now - RETENTION_MS - 1;
        let fresh_alert_at = now - 1_000;
        db.write_tx(|tx| {
            tx.execute(
                "INSERT INTO alerts
                   (server_id, server_name, rule_id, severity, status, title, detail,
                    first_seen_at, last_seen_at)
                 VALUES ('srv_1', 'web-01', 'stale-rule', 'warning', 'active', 'stale', '',
                         ?1, ?1)",
                [stale_alert_at],
            )
            .map_err(DbError::write)?;
            tx.execute(
                "INSERT INTO alerts
                   (server_id, server_name, rule_id, severity, status, title, detail,
                    first_seen_at, last_seen_at)
                 VALUES ('srv_1', 'web-01', 'fresh-rule', 'warning', 'active', 'fresh', '',
                         ?1, ?1)",
                [fresh_alert_at],
            )
            .map_err(DbError::write)?;
            tx.execute(
                "INSERT INTO alert_mutes (server_id, rule_id, until_ms, created_at)
                 VALUES ('srv_1', NULL, 10, 1)",
                [],
            )
            .map_err(DbError::write)?;
            tx.execute(
                "INSERT INTO login_sources (server_id, ip, first_seen_at, last_seen_at)
                 VALUES ('srv_1', '203.0.113.9', 1, 1)",
                [],
            )
            .map_err(DbError::write)?;
            Ok(())
        })
        .unwrap();

        db.prune_retention(now).unwrap();

        assert!(
            db.audit_get(expired).unwrap().is_none(),
            "行数未超上限时仍按 30 天窗口删除旧行"
        );
        assert_eq!(
            db.audit_list(RETENTION_ROWS + 20, 0).unwrap().len(),
            (RETENTION_ROWS - 1) as usize
        );

        let alerts: Vec<(String, i64)> = {
            let conn = db.lock().unwrap();
            let mut statement = conn
                .prepare("SELECT rule_id, first_seen_at FROM alerts ORDER BY first_seen_at DESC")
                .unwrap();
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap();
            rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        assert_eq!(
            alerts,
            vec![("fresh-rule".to_string(), fresh_alert_at)],
            "只应删除超过 30 天的告警"
        );

        for index in (RETENTION_ROWS - 1)..(RETENTION_ROWS + 10) {
            db.audit_insert(&entry(now - 30_000 + index)).unwrap();
        }
        db.prune_retention(now).unwrap();

        let audits = db.audit_list(RETENTION_ROWS + 20, 0).unwrap();
        assert_eq!(audits.len(), RETENTION_ROWS as usize);
        assert_eq!(audits[0].at_ms, now - 30_000 + RETENTION_ROWS + 9);
        let mutes: i64 = db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM alert_mutes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mutes, 0);
        let sources: i64 = db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM login_sources", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sources, 0);
    }
}
