pub mod cache;
pub mod download;
pub mod manifest;
pub mod source;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::DbState;

const MAX_ASSET_BYTES: u64 = cache::MAX_ASSET_BYTES;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub date: Option<String>,
    pub body: Option<String>,
    pub size: Option<u64>,
    pub url: Option<String>,
    pub cached: bool,
    pub deferred: bool,
}

#[derive(Clone)]
struct Pending {
    raw: Value,
    asset_url: reqwest::Url,
}

struct Inner {
    cancel: Option<Arc<AtomicBool>>,
    pending: Option<Pending>,
}

pub struct UpdateService {
    client: reqwest::Client,
    inner: Mutex<Inner>,
}

/// 跳转被拒绝时返回给 reqwest 的错误类型，让下载直接失败而不是静默降级。
#[derive(Debug)]
struct RedirectRejected(&'static str);

impl std::fmt::Display for RedirectRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for RedirectRejected {}

/// 只跟随仍是 HTTPS 的跳转：镜像或代理可能把请求 302 到明文地址。
fn redirect_target_allowed(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
}

fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if !redirect_target_allowed(attempt.url()) {
            return attempt.error(RedirectRejected("拒绝跟随非 HTTPS 跳转"));
        }
        // 手工限链：不能用 `Policy::limited`，否则会覆盖上面的 HTTPS 判断。
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error(RedirectRejected("跳转次数过多"));
        }
        attempt.follow()
    })
}

impl UpdateService {
    pub fn new() -> Self {
        // reqwest 以 rustls-no-provider 编译（与 tauri-plugin-updater 对齐），进程内必须显式
        // 装一次 ring provider，否则首个 HTTPS 请求会 panic "No rustls crypto provider"。
        // 已装过（例如别的库先装）时返回 Err，忽略即可。
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .user_agent(format!("LightGuard/{}", env!("CARGO_PKG_VERSION")))
            // 不设整请求超时：7 MiB 的资产在慢网下会超过 30 秒；用连接/单次读取超时兜底。
            .connect_timeout(std::time::Duration::from_secs(15))
            .read_timeout(std::time::Duration::from_secs(60))
            .redirect(redirect_policy())
            .build()
            .expect("build update http client");
        Self {
            client,
            inner: Mutex::new(Inner {
                cancel: None,
                pending: None,
            }),
        }
    }
}

async fn cached_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?;
    let dir = cache::cache_dir(&local_root);
    // 读不出缓存（缺失、损坏或超限）时顺手清一次，避免坏包长期占用磁盘。
    let Some((meta, bytes)) = cache::read_cache(&dir) else {
        cache::clear_cache(&dir);
        return Ok(None);
    };
    let current = app.package_info().version.to_string();
    if !manifest::is_newer(&current, &meta.version) {
        cache::clear_cache(&dir);
        return Ok(None);
    }
    // 缓存损坏只应丢弃缓存并回到联网检查，不能把“检查更新”整体变成错误。
    let Ok(update) = cache::restore_update(&app, meta.raw_manifest.clone()) else {
        cache::clear_cache(&dir);
        return Ok(None);
    };
    if update.verify(&bytes).is_err() {
        cache::clear_cache(&dir);
        return Ok(None);
    }
    let Ok(parsed) = manifest::parse_release(&meta.raw_manifest) else {
        cache::clear_cache(&dir);
        return Ok(None);
    };
    Ok(Some(UpdateInfo {
        version: meta.version,
        current_version: current,
        date: parsed.pub_date,
        body: parsed.notes,
        size: Some(bytes.len() as u64),
        url: Some(parsed.asset_url.to_string()),
        cached: true,
        deferred: meta.deferred,
    }))
}

#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    service: State<'_, UpdateService>,
    db: State<'_, DbState>,
) -> Result<Option<UpdateInfo>, String> {
    if let Some(info) = cached_update(app.clone()).await? {
        return Ok(Some(info));
    }
    let selected = db
        .handle()
        .and_then(|database| database.settings_raw().ok().flatten())
        .and_then(|value| {
            value
                .get("updateSource")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| source::AUTO.to_string());
    let sources = source::resolve_sources(&selected)?;
    let current = app.package_info().version.to_string();

    let mut last_error = String::from("没有可用的更新源");
    let mut up_to_date = false;
    for url in sources {
        let response = match service
            .client
            .get(url.clone())
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                last_error = format!("请求 {url} 失败：{error}");
                continue;
            }
        };
        if !response.status().is_success() {
            last_error = format!("请求 {url} 失败：HTTP {}", response.status());
            continue;
        }
        let raw: Value = match response.json().await {
            Ok(raw) => raw,
            Err(error) => {
                last_error = format!("解析 {url} 失败：{error}");
                continue;
            }
        };
        let parsed = match manifest::parse_release(&raw) {
            Ok(parsed) => parsed,
            Err(error) => {
                last_error = error;
                continue;
            }
        };
        if !manifest::is_newer(&current, &parsed.version) {
            // 某个镜像可能缓存了旧清单，继续看后面的源，避免漏掉真正的新版本。
            up_to_date = true;
            continue;
        }
        let info = UpdateInfo {
            version: parsed.version.clone(),
            current_version: current.clone(),
            date: parsed.pub_date.clone(),
            body: parsed.notes.clone(),
            size: parsed.platform.size,
            url: Some(parsed.asset_url.to_string()),
            cached: false,
            deferred: false,
        };
        service.inner.lock().unwrap().pending = Some(Pending {
            raw,
            asset_url: parsed.asset_url,
        });
        return Ok(Some(info));
    }
    if up_to_date {
        return Ok(None);
    }
    Err(last_error)
}

// 只清自己那一轮的取消句柄：取消后立刻重开下载时，旧轮次不能把新轮的标志清掉。
fn clear_cancel(service: &UpdateService, cancel: &Arc<AtomicBool>) {
    let mut inner = service.inner.lock().unwrap();
    if inner
        .cancel
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, cancel))
    {
        inner.cancel = None;
    }
}

#[tauri::command]
pub async fn update_download(
    app: AppHandle,
    service: State<'_, UpdateService>,
) -> Result<(), String> {
    let (pending, cancel) = {
        let mut inner = service.inner.lock().unwrap();
        if inner.cancel.is_some() {
            return Err("已有下载在进行".to_string());
        }
        let pending = inner
            .pending
            .clone()
            .ok_or_else(|| "没有待下载的更新".to_string())?;
        let cancel = Arc::new(AtomicBool::new(false));
        inner.cancel = Some(cancel.clone());
        (pending, cancel)
    };

    let candidates = manifest::asset_candidates(&pending.asset_url);
    let app_for_progress = app.clone();
    let result = download::download_asset(
        &service.client,
        &candidates,
        MAX_ASSET_BYTES,
        cancel.clone(),
        move |progress| {
            let _ = app_for_progress.emit("update://progress", progress);
        },
    )
    .await;
    // 无论成功失败都释放本轮句柄；ptr_eq 保证不会误清新一轮下载。
    clear_cancel(&service, &cancel);

    let (bytes, source) = match result {
        Ok(value) => value,
        Err(download::DownloadError::Cancelled) => return Err("已取消下载".to_string()),
        Err(download::DownloadError::Failed(error)) => return Err(error),
    };

    let update = cache::restore_update(&app, pending.raw.clone())?;
    update
        .verify(&bytes)
        .map_err(|error| format!("更新包验签失败：{error}"))?;

    // 清单已经解析过一次，这里重新解析拿纯 semver 版本号：`write_cache` 会用它做文件名，
    // 并要求清单顶层 `version` 与之完全一致。
    let parsed = manifest::parse_release(&pending.raw)?;
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?;
    let dir = cache::cache_dir(&local_root);
    // 写新版本前先清空：旧版本的包不能留在缓存里被下次启动误认。
    cache::clear_cache(&dir);
    cache::write_cache(
        &dir,
        &parsed.version,
        &pending.raw,
        &source.to_string(),
        &bytes,
        false,
    )?;
    Ok(())
}

#[tauri::command]
pub fn update_cancel(service: State<'_, UpdateService>) -> Result<(), String> {
    // 取走句柄而不是只置位：取消后用户可能立刻重开下载，不能因为旧轮次还没退出就被“已有下载在进行”挡住。
    let cancel = service.inner.lock().unwrap().cancel.take();
    if let Some(cancel) = cancel {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub fn update_defer(app: AppHandle) -> Result<(), String> {
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?;
    let dir = cache::cache_dir(&local_root);
    // 没有缓存时“稍后提醒”无处落盘，按无操作返回：否则前端会为一个无害的点击收到错误。
    if cache::read_cache(&dir).is_none() {
        return Ok(());
    }
    cache::set_deferred(&dir, true)
}

#[tauri::command]
pub fn update_install(app: AppHandle, version: String) -> Result<(), String> {
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?;
    let dir = cache::cache_dir(&local_root);
    let Some((meta, bytes)) = cache::read_cache(&dir) else {
        return Err("没有已下载的更新包".to_string());
    };
    if meta.version != version {
        return Err("更新包版本不匹配".to_string());
    }
    let update = cache::restore_update(&app, meta.raw_manifest)?;
    // Windows 上 install 成功会直接退出进程；缓存在下次启动时按版本号清理。
    // `install` 内部会再验一次签名，所以这里不需要重复调用 `verify`。
    update
        .install(&bytes)
        .map_err(|error| format!("安装更新失败：{error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_builds_with_ring_provider() {
        // 缺少 rustls provider 时构建 reqwest 客户端会在首次 TLS 使用时 panic；
        // 这里直接构造并丢弃，保证 `new()` 自己把 provider 装好。
        let service = UpdateService::new();
        drop(service);
    }

    #[test]
    fn redirects_must_stay_https() {
        let https = reqwest::Url::parse("https://github.com/x").unwrap();
        let plain = reqwest::Url::parse("http://github.com/x").unwrap();
        assert!(redirect_target_allowed(&https));
        assert!(!redirect_target_allowed(&plain));
    }
}
