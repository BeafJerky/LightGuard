use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

pub const MAX_ASSET_BYTES: u64 = 100 * 1024 * 1024;
const META_FILE: &str = "cache.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheMeta {
    pub version: String,
    pub raw_manifest: Value,
    pub downloaded_at: i64,
    pub source_url: String,
    #[serde(default)]
    pub deferred: bool,
}

pub fn cache_dir(local_root: &Path) -> PathBuf {
    local_root.join("updates")
}

pub fn cache_paths(dir: &Path, version: &str) -> (PathBuf, PathBuf) {
    (
        dir.join(format!("LightGuard_{version}_x64-setup.exe")),
        dir.join(META_FILE),
    )
}

pub fn write_cache(
    dir: &Path,
    version: &str,
    raw: &Value,
    source_url: &str,
    bytes: &[u8],
    deferred: bool,
) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| format!("创建更新缓存目录失败：{error}"))?;
    let (exe, meta) = cache_paths(dir, version);
    let exe_tmp = exe.with_extension("exe.tmp");
    let meta_tmp = meta.with_extension("json.tmp");
    fs::write(&exe_tmp, bytes).map_err(|error| format!("写入更新包失败：{error}"))?;
    let _ = fs::remove_file(&exe);
    fs::rename(&exe_tmp, &exe).map_err(|error| format!("落位更新包失败：{error}"))?;
    let payload = CacheMeta {
        version: version.to_string(),
        raw_manifest: raw.clone(),
        downloaded_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
        source_url: source_url.to_string(),
        deferred,
    };
    let json =
        serde_json::to_vec(&payload).map_err(|error| format!("序列化缓存元数据失败：{error}"))?;
    fs::write(&meta_tmp, json).map_err(|error| format!("写入缓存元数据失败：{error}"))?;
    let _ = fs::remove_file(&meta);
    fs::rename(&meta_tmp, &meta).map_err(|error| format!("落位缓存元数据失败：{error}"))?;
    Ok(())
}

pub fn read_cache(dir: &Path) -> Option<(CacheMeta, Vec<u8>)> {
    let raw_meta = fs::read(dir.join(META_FILE)).ok()?;
    let meta: CacheMeta = serde_json::from_slice(&raw_meta).ok()?;
    let (exe, _) = cache_paths(dir, &meta.version);
    let bytes = fs::read(exe).ok()?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_ASSET_BYTES {
        return None;
    }
    Some((meta, bytes))
}

pub fn set_deferred(dir: &Path, deferred: bool) -> Result<(), String> {
    let raw_meta =
        fs::read(dir.join(META_FILE)).map_err(|error| format!("读取缓存元数据失败：{error}"))?;
    let mut meta: CacheMeta = serde_json::from_slice(&raw_meta)
        .map_err(|error| format!("解析缓存元数据失败：{error}"))?;
    meta.deferred = deferred;
    let json =
        serde_json::to_vec(&meta).map_err(|error| format!("序列化缓存元数据失败：{error}"))?;
    let meta_path = dir.join(META_FILE);
    let tmp = meta_path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|error| format!("写入缓存元数据失败：{error}"))?;
    let _ = fs::remove_file(&meta_path);
    fs::rename(&tmp, &meta_path).map_err(|error| format!("落位缓存元数据失败：{error}"))
}

pub fn clear_cache(dir: &Path) {
    let meta_tmp = format!("{META_FILE}.tmp");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // 一并清掉 `.exe.tmp` / `cache.json.tmp`，中断的写入不会占着磁盘。
            if name == META_FILE || name == meta_tmp || name.starts_with("LightGuard_") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

pub fn restore_update(app: &AppHandle, raw: Value) -> Result<Update, String> {
    let updater = app
        .updater_builder()
        .build()
        .map_err(|error| format!("初始化更新器失败：{error}"))?;
    updater
        .restore_update(raw)
        .map_err(|error| format!("恢复更新清单失败：{error}"))?
        .ok_or_else(|| "缓存清单与当前版本不匹配".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn writes_reads_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let raw = json!({ "version": "0.15.0" });
        write_cache(
            dir.path(),
            "0.15.0",
            &raw,
            "https://example.com/a.exe",
            b"abc",
            false,
        )
        .unwrap();

        let (meta, bytes) = read_cache(dir.path()).unwrap();
        assert_eq!(meta.version, "0.15.0");
        assert_eq!(meta.source_url, "https://example.com/a.exe");
        assert!(!meta.deferred);
        assert_eq!(bytes, b"abc");

        set_deferred(dir.path(), true).unwrap();
        assert!(read_cache(dir.path()).unwrap().0.deferred);

        clear_cache(dir.path());
        assert!(read_cache(dir.path()).is_none());
    }

    #[test]
    fn ignores_incomplete_cache() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cache.json"), b"{}").unwrap();
        assert!(read_cache(dir.path()).is_none());
    }

    #[test]
    fn cache_paths_are_versioned() {
        let (exe, meta) = cache_paths(std::path::Path::new("x"), "0.15.0");
        assert!(exe.ends_with("LightGuard_0.15.0_x64-setup.exe"));
        assert!(meta.ends_with("cache.json"));
    }

    #[test]
    fn clear_cache_removes_interrupted_writes() {
        let dir = tempfile::tempdir().unwrap();
        let raw = json!({ "version": "0.15.0" });
        write_cache(
            dir.path(),
            "0.15.0",
            &raw,
            "https://example.com/a.exe",
            b"abc",
            false,
        )
        .unwrap();
        let (exe, meta) = cache_paths(dir.path(), "0.15.0");
        let exe_tmp = exe.with_extension("exe.tmp");
        let meta_tmp = meta.with_extension("json.tmp");
        std::fs::write(&exe_tmp, b"partial").unwrap();
        std::fs::write(&meta_tmp, b"{").unwrap();

        clear_cache(dir.path());

        assert!(!exe.exists());
        assert!(!meta.exists());
        assert!(!exe_tmp.exists());
        assert!(!meta_tmp.exists());
    }
}
