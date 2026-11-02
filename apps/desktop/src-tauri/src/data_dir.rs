use std::fs;
use std::path::{Path, PathBuf};

pub const DB_DIR_NAME: &str = "data";
const DB_FILE: &str = "lightguard.db";
const SIDECARS: [&str; 3] = ["lightguard.db", "lightguard.db-wal", "lightguard.db-shm"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbLocation {
    pub dir: PathBuf,
    pub migrated: bool,
    pub warning: Option<String>,
}

pub fn resolve_db_dir(local_root: &Path, legacy_root: &Path) -> DbLocation {
    let target = local_root.join(DB_DIR_NAME);
    if target.join(DB_FILE).exists() {
        return DbLocation { dir: target, migrated: false, warning: None };
    }
    if !legacy_root.join(DB_FILE).exists() {
        return DbLocation { dir: target, migrated: false, warning: None };
    }
    match migrate(legacy_root, &target) {
        Ok(()) => DbLocation { dir: target, migrated: true, warning: None },
        Err(error) => DbLocation {
            dir: legacy_root.to_path_buf(),
            migrated: false,
            warning: Some(format!("数据库迁移失败，继续使用旧目录：{error}")),
        },
    }
}

fn migrate(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|error| format!("创建目标目录失败：{error}"))?;
    let staging = dst.join("migrating");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|error| format!("创建暂存目录失败：{error}"))?;

    // 先把 WAL 合并进主库；失败也不影响后续整组复制（-wal/-shm 一起带走）。
    if let Ok(conn) = rusqlite::Connection::open(src.join(DB_FILE)) {
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }

    for name in SIDECARS {
        let from = src.join(name);
        if from.exists() {
            fs::copy(&from, staging.join(name))
                .map_err(|error| format!("复制 {name} 失败：{error}"))?;
        }
    }
    verify_sqlite(&staging.join(DB_FILE))?;

    // 目标目录可能是上次中断留下的；先清掉旧文件，避免把旧的 -wal 和新主库配成一对。
    for name in SIDECARS {
        let _ = fs::remove_file(dst.join(name));
    }
    for name in SIDECARS {
        let from = staging.join(name);
        if from.exists() {
            fs::rename(&from, dst.join(name))
                .map_err(|error| format!("落位 {name} 失败：{error}"))?;
        }
    }
    let _ = fs::remove_dir_all(&staging);

    // 只有新库验证通过并落位后才清旧文件；删除失败不影响使用。
    for name in SIDECARS {
        let _ = fs::remove_file(src.join(name));
    }
    Ok(())
}

fn verify_sqlite(path: &Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open(path)
        .map_err(|error| format!("打开迁移副本失败：{error}"))?;
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| format!("完整性检查失败：{error}"))?;
    if result != "ok" {
        return Err(format!("完整性检查未通过：{result}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::tempdir;

    fn seed_legacy(dir: &Path) -> Connection {
        std::fs::create_dir_all(dir).unwrap();
        let conn = Connection::open(dir.join("lightguard.db")).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        conn.execute_batch(
            "CREATE TABLE t (v TEXT);\
             INSERT INTO t VALUES ('kept');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn migrates_legacy_database_and_keeps_rows() {
        let local = tempdir().unwrap();
        let old = tempdir().unwrap();
        // 先关闭连接：Windows 上持有打开的 SQLite 句柄会让后续改名/删除不稳定。
        drop(seed_legacy(old.path()));

        let location = resolve_db_dir(local.path(), old.path());

        assert!(location.migrated);
        assert_eq!(location.dir, local.path().join("data"));
        assert!(location.warning.is_none());
        assert!(!old.path().join("lightguard.db").exists());

        let conn = Connection::open(local.path().join("data/lightguard.db")).unwrap();
        let value: String = conn
            .query_row("SELECT v FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "kept");
    }

    #[test]
    fn keeps_fresh_install_in_local_app_data() {
        let local = tempdir().unwrap();
        let old = tempdir().unwrap();
        let location = resolve_db_dir(local.path(), old.path());
        assert!(!location.migrated);
        assert_eq!(location.dir, local.path().join("data"));
    }

    #[test]
    fn prefers_an_existing_local_database() {
        let local = tempdir().unwrap();
        let old = tempdir().unwrap();
        std::fs::create_dir_all(local.path().join("data")).unwrap();
        Connection::open(local.path().join("data/lightguard.db")).unwrap();
        seed_legacy(old.path());

        let location = resolve_db_dir(local.path(), old.path());

        assert!(!location.migrated);
        assert!(old.path().join("lightguard.db").exists());
    }

    #[test]
    fn falls_back_to_legacy_when_target_cannot_be_created() {
        let local = tempdir().unwrap();
        let old = tempdir().unwrap();
        std::fs::write(local.path().join("data"), b"not a directory").unwrap();
        seed_legacy(old.path());

        let location = resolve_db_dir(local.path(), old.path());

        assert!(!location.migrated);
        assert_eq!(location.dir, old.path());
        assert!(location.warning.is_some());
        assert!(old.path().join("lightguard.db").exists());
    }

    #[test]
    fn second_run_is_idempotent() {
        let local = tempdir().unwrap();
        let old = tempdir().unwrap();
        drop(seed_legacy(old.path()));

        let first = resolve_db_dir(local.path(), old.path());
        let second = resolve_db_dir(local.path(), old.path());

        assert!(first.migrated);
        assert!(!second.migrated);
        assert_eq!(second.dir, first.dir);
    }
}
