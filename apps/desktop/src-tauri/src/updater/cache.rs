use std::fs::{self, File};
use std::io::Write;
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

/// 先写临时文件并 `sync_all`，再落位；断电时最多丢掉临时文件，不会留下被 `read_cache`
/// 误认的半截文件。
fn write_synced(path: &Path, bytes: &[u8], context: &str) -> Result<(), String> {
    let mut file = File::create(path).map_err(|error| format!("{context}：{error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("{context}：{error}"))?;
    file.sync_all()
        .map_err(|error| format!("{context}：{error}"))?;
    Ok(())
}

pub fn write_cache(
    dir: &Path,
    version: &str,
    raw: &Value,
    source_url: &str,
    bytes: &[u8],
    deferred: bool,
) -> Result<(), String> {
    // 版本号会成为文件名，先做语义化校验，避免非法字符与 `..` 路径穿越。
    semver::Version::parse(version).map_err(|_| "更新版本号无效".to_string())?;
    if bytes.is_empty() {
        return Err("更新包为空".to_string());
    }
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err("更新包超过大小上限".to_string());
    }
    fs::create_dir_all(dir).map_err(|error| format!("创建更新缓存目录失败：{error}"))?;
    let (exe, meta) = cache_paths(dir, version);
    let exe_tmp = exe.with_extension("exe.tmp");
    let meta_tmp = meta.with_extension("json.tmp");
    write_synced(&exe_tmp, bytes, "写入更新包失败")?;
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
    write_synced(&meta_tmp, &json, "写入缓存元数据失败")?;
    let _ = fs::remove_file(&meta);
    fs::rename(&meta_tmp, &meta).map_err(|error| format!("落位缓存元数据失败：{error}"))?;
    Ok(())
}

pub fn read_cache(dir: &Path) -> Option<(CacheMeta, Vec<u8>)> {
    let raw_meta = fs::read(dir.join(META_FILE)).ok()?;
    let meta: CacheMeta = serde_json::from_slice(&raw_meta).ok()?;
    // 版本号会拼进缓存文件名：先做语义化校验，挡住被手改的 `..\..\evil` 之类路径穿越，
    // 而不能等到 `cache_paths` 里才发现。
    semver::Version::parse(&meta.version).ok()?;
    // 清单与元数据版本不一致说明缓存与清单已经对不上，按无缓存处理。
    if meta.raw_manifest.get("version").and_then(Value::as_str) != Some(meta.version.as_str()) {
        return None;
    }
    let (exe, _) = cache_paths(dir, &meta.version);
    let size = fs::metadata(&exe).ok()?.len();
    if size == 0 || size > MAX_ASSET_BYTES {
        return None;
    }
    let bytes = fs::read(&exe).ok()?;
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
    write_synced(&tmp, &json, "写入缓存元数据失败")?;
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

/// 调用方必须先注册 `tauri_plugin_updater`，否则 `updater_builder()` 会 panic。
pub fn restore_update(app: &AppHandle, raw: Value) -> Result<Update, String> {
    let updater = app
        .updater_builder()
        .build()
        .map_err(|error| format!("初始化更新器失败：{error}"))?;
    updater
        .restore_update(raw)
        .map_err(|error| format!("恢复更新清单失败：{error}"))?
        .ok_or_else(|| "缓存清单已不提供更新".to_string())
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
    fn rejects_invalid_version() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("updates");
        let raw = json!({ "version": "0.15.0" });

        let error = write_cache(
            &cache,
            "0.15.0/../evil",
            &raw,
            "https://example.com/a.exe",
            b"abc",
            false,
        )
        .unwrap_err();

        assert!(error.contains("更新版本号无效"), "unexpected error: {error}");
        // 既不能落盘更新包，也不能因版本号里的 `..` 写到缓存目录之外。
        assert!(!dir.path().join("evil_x64-setup.exe").exists());
        let leftover = fs::read_dir(&cache)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|entry| entry.file_name().to_string_lossy().starts_with("LightGuard_"))
            })
            .unwrap_or(false);
        assert!(!leftover);
    }

    #[test]
    fn rejects_oversized_payload() {
        let dir = tempfile::tempdir().unwrap();
        let raw = json!({ "version": "0.15.0" });
        let bytes = vec![0_u8; MAX_ASSET_BYTES as usize + 1];

        let error = write_cache(
            dir.path(),
            "0.15.0",
            &raw,
            "https://example.com/a.exe",
            &bytes,
            false,
        )
        .unwrap_err();

        assert!(error.contains("更新包"), "unexpected error: {error}");
        let (exe, meta) = cache_paths(dir.path(), "0.15.0");
        assert!(!exe.exists());
        assert!(!meta.exists());
    }

    #[test]
    fn ignores_cache_with_mismatched_manifest_version() {
        let dir = tempfile::tempdir().unwrap();
        let meta = json!({
            "version": "0.15.0",
            "rawManifest": { "version": "0.16.0" },
            "downloadedAt": 1,
            "sourceUrl": "https://example.com/a.exe",
            "deferred": false,
        });
        std::fs::write(
            dir.path().join("cache.json"),
            serde_json::to_vec(&meta).unwrap(),
        )
        .unwrap();
        let (exe, _) = cache_paths(dir.path(), "0.15.0");
        std::fs::write(&exe, b"abc").unwrap();

        assert!(read_cache(dir.path()).is_none());
    }

    #[test]
    fn rejects_cache_with_traversing_version() {
        let dir = tempfile::tempdir().unwrap();
        let version = "..\\..\\evil";
        let meta = json!({
            "version": version,
            // 清单版本故意写成一致，确保拦截来自 semver 校验而不是版本不一致分支。
            "rawManifest": { "version": version },
            "downloadedAt": 1,
            "sourceUrl": "https://example.com/a.exe",
            "deferred": false,
        });
        std::fs::write(
            dir.path().join("cache.json"),
            serde_json::to_vec(&meta).unwrap(),
        )
        .unwrap();
        let (exe, _) = cache_paths(dir.path(), "0.15.0");
        std::fs::write(&exe, b"abc").unwrap();

        assert!(read_cache(dir.path()).is_none());
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
