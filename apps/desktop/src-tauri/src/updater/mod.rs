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
/// 进度事件的最小间隔：7 MiB 的包在快网下能产生上百个分块，逐个转发会把 IPC 打满。
const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

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
    /// 下载轮次代号：`begin_download` / `cancel_download` 都会递增它，旧轮次据此放弃写缓存。
    generation: u64,
    cancel: Option<Arc<AtomicBool>>,
    pending: Option<Pending>,
}

impl Inner {
    fn new() -> Self {
        Self {
            generation: 0,
            cancel: None,
            pending: None,
        }
    }
}

/// 开始一轮下载：拒绝并发下载，递增代号并登记本轮取消句柄。
/// 代号与句柄必须在同一把锁里落定，否则两轮下载会同时写缓存而互相覆盖。
fn begin_download(inner: &mut Inner) -> Result<(u64, Arc<AtomicBool>), String> {
    if inner.cancel.is_some() {
        return Err("已有下载在进行".to_string());
    }
    inner.generation = inner.generation.wrapping_add(1);
    let generation = inner.generation;
    let cancel = Arc::new(AtomicBool::new(false));
    inner.cancel = Some(cancel.clone());
    Ok((generation, cancel))
}

/// 取消当前轮次：取走句柄并置位，同时递增代号让旧轮次失去写缓存的资格。
/// 代号一递增，用户就能立刻重开下载，不必等旧轮次的 future 退出。
fn cancel_download(inner: &mut Inner) {
    inner.generation = inner.generation.wrapping_add(1);
    if let Some(cancel) = inner.cancel.take() {
        cancel.store(true, Ordering::Relaxed);
    }
}

/// 结束一轮下载，返回“仍是当前轮次、可以提交缓存”。
/// 只有 `begin_download` 会登记句柄且它一定递增代号，所以代号相等即说明句柄属于本轮，
/// 这里不会误清新一轮下载的句柄。
fn finish_download(inner: &mut Inner, generation: u64) -> bool {
    if inner.generation != generation {
        return false;
    }
    inner.cancel = None;
    true
}

/// 进度节流：距上次发送不足 `PROGRESS_INTERVAL` 时跳过，但已知总量且已下满的最后一块
/// 一定发送，避免前端停在上一个刻度上。
fn progress_due(last: Option<std::time::Instant>, progress: download::DownloadProgress) -> bool {
    if progress
        .total
        .is_some_and(|total| progress.downloaded >= total)
    {
        return true;
    }
    match last {
        Some(last) => last.elapsed() >= PROGRESS_INTERVAL,
        None => true,
    }
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
            inner: Mutex::new(Inner::new()),
        }
    }

    /// 中毒的锁只代表某次 panic 时还在持锁，状态本身仍是自洽的：恢复内部值继续用，
    /// 否则一次 panic 会让“检查更新”永久失效。
    fn lock_inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

async fn cached_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?;
    let dir = cache::cache_dir(&local_root);
    // 读不出缓存（从未下载或读盘失败）时不能顺手清目录：刚写好的包可能只是暂时读不到，
    // 清掉会让用户白下一次。真正的坏包在下面各自的分支里清。
    let Some((meta, bytes)) = cache::read_cache(&dir) else {
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
    // 存下来的更新源可能是旧版本写坏的（非 HTTPS、带凭据、手改乱码）。这时退回内置默认源，
    // 不能因为一个坏设置让“检查更新”整体不可用。
    let sources = match source::resolve_sources(&selected) {
        Ok(sources) => sources,
        Err(_) => source::resolve_sources(source::AUTO)?,
    };
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
        service.lock_inner().pending = Some(Pending {
            raw,
            asset_url: parsed.asset_url,
        });
        return Ok(Some(info));
    }
    if up_to_date {
        // 所有源都说没有更新：之前发现的待下载更新已经过时，留着只会让 `update_download`
        // 去下载一个已不再发布的版本。全部源失败时（下面的 Err 分支）不清，保留可下载状态。
        service.lock_inner().pending = None;
        return Ok(None);
    }
    Err(last_error)
}

#[tauri::command]
pub async fn update_download(
    app: AppHandle,
    service: State<'_, UpdateService>,
) -> Result<(), String> {
    let (pending, generation, cancel) = {
        let mut inner = service.lock_inner();
        let pending = inner
            .pending
            .clone()
            .ok_or_else(|| "没有待下载的更新".to_string())?;
        let (generation, cancel) = begin_download(&mut inner)?;
        (pending, generation, cancel)
    };

    let candidates = manifest::asset_candidates(&pending.asset_url);
    let app_for_progress = app.clone();
    let mut last_emit: Option<std::time::Instant> = None;
    let result = download::download_asset(
        &service.client,
        &candidates,
        MAX_ASSET_BYTES,
        cancel,
        move |progress| {
            if progress_due(last_emit, progress) {
                last_emit = Some(std::time::Instant::now());
                let _ = app_for_progress.emit("update://progress", progress);
            }
        },
    )
    .await;

    // 本轮是否仍被认账：取消或被新一轮顶替后，即使下载本身返回 Ok 也不能落盘，
    // 否则会覆盖新一轮正在写的缓存。
    let still_current = {
        let mut inner = service.lock_inner();
        finish_download(&mut inner, generation)
    };
    if !still_current {
        return Err("已取消下载".to_string());
    }

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
    // 包已经落盘：待下载状态作废，再点“检查更新”应该走缓存的安装路径。
    service.lock_inner().pending = None;
    Ok(())
}

#[tauri::command]
pub fn update_cancel(service: State<'_, UpdateService>) -> Result<(), String> {
    let mut inner = service.lock_inner();
    cancel_download(&mut inner);
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
    // 包已经在内存里，安装要么让进程退出要么失败：先把磁盘上的旧包清掉，
    // 否则一个过期的大包会一直占着几十上百 MB。
    cache::clear_cache(&dir);
    let update = cache::restore_update(&app, meta.raw_manifest)?;
    // Windows 上 install 成功会直接退出进程；失败时缓存已经清空，用户需要重新下载。
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

    #[test]
    fn update_cancel_supersedes_a_running_download() {
        // 命令层要 AppHandle，但轮次记账是纯状态机，可以直接在 `Inner` 上验证。
        let mut inner = Inner::new();
        let (first, first_cancel) = begin_download(&mut inner).unwrap();
        assert!(!first_cancel.load(Ordering::Relaxed));
        // 两轮并发会让临时文件互相覆盖，第二轮必须被挡住。
        assert!(begin_download(&mut inner).is_err());

        cancel_download(&mut inner);
        assert!(first_cancel.load(Ordering::Relaxed));
        // 旧轮次回来时已不是当前轮次：不能提交缓存。
        assert!(!finish_download(&mut inner, first));

        // 取消后可以立刻重开：代号已经递增，不会被“已有下载在进行”挡住。
        let (second, second_cancel) = begin_download(&mut inner).unwrap();
        assert_ne!(second, first);
        // 旧轮次迟到的 finish 不能清掉新轮次的句柄，也不能获得提交权。
        assert!(!finish_download(&mut inner, first));
        assert!(inner.cancel.is_some());
        assert!(!second_cancel.load(Ordering::Relaxed));

        // 新轮次正常收尾：拿到提交权并释放句柄。
        assert!(finish_download(&mut inner, second));
        assert!(inner.cancel.is_none());
        // 收尾后可以开下一轮。
        assert!(begin_download(&mut inner).is_ok());
    }

    #[test]
    fn progress_is_throttled_but_final_chunk_always_emits() {
        let mid = download::DownloadProgress {
            downloaded: 10,
            total: Some(100),
        };
        // 没有上次发送记录时立刻发。
        assert!(progress_due(None, mid));
        // 刚发过就跳过，避免每个分块都过 IPC。
        assert!(!progress_due(Some(std::time::Instant::now()), mid));
        // 已下满时无论是否刚发过都补最后一帧。
        let done = download::DownloadProgress {
            downloaded: 100,
            total: Some(100),
        };
        assert!(progress_due(Some(std::time::Instant::now()), done));
        // 总量未知（无 Content-Length）时只能按时间节流。
        let unknown = download::DownloadProgress {
            downloaded: 100,
            total: None,
        };
        assert!(!progress_due(Some(std::time::Instant::now()), unknown));
    }
}
