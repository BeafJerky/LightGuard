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

/// 释放某一轮登记的取消句柄，返回“仍是当前轮次”。
/// 只有 `begin_download` 会登记句柄且它一定递增代号，所以代号相等即说明句柄属于本轮，
/// 这里不会误清新一轮下载的句柄；已被顶替时由取消方或新一轮自己负责。
fn release_download(inner: &mut Inner, generation: u64) -> bool {
    if inner.generation != generation {
        return false;
    }
    inner.cancel = None;
    true
}

/// 在持锁状态下提交缓存：先按代号判定本轮是否仍有效，再把包写进缓存目录。
/// 判定与写盘必须在同一把锁里完成——否则一轮可能先通过判定，再被新一轮顶替，
/// 两轮交替写盘把缓存写坏。`update_cancel` 因此会阻塞到提交结束，这是刻意的：
/// 取消先递增代号，下一轮只会在提交之后才开始，`clear_cache` 的先后顺序仍然安全。
fn commit_download(
    inner: &mut Inner,
    generation: u64,
    version: &str,
    pending: &Pending,
    source_url: &str,
    dir: &std::path::Path,
    bytes: &[u8],
) -> Result<(), String> {
    if inner.generation != generation {
        // 已被取消或顶替：碰都不能碰磁盘，否则会覆盖新一轮正在写的缓存。
        return Err("已取消下载".to_string());
    }
    // 写新版本前先清空：旧版本的包不能留在缓存里被下次启动误认。
    cache::clear_cache(dir);
    let written = cache::write_cache(dir, version, &pending.raw, source_url, bytes, false);
    // 落盘结束（成败都算）才放开这一轮：失败时不放开，用户就永远无法重试。
    inner.cancel = None;
    written?;
    // 包已经落盘：待下载状态作废，再点“检查更新”应该走缓存的安装路径。
    // 但下载期间的一次“检查更新”可能已经把待下载换成了更新的版本，那种情况要留着。
    let same_version = inner.pending.as_ref().is_some_and(|current| {
        manifest::parse_release(&current.raw).is_ok_and(|parsed| parsed.version == version)
    });
    if same_version {
        inner.pending = None;
    }
    Ok(())
}

/// 进度节流：距上次发送不足 `PROGRESS_INTERVAL` 时跳过，但已知总量且已下满的最后一块
/// 一定发送，避免前端停在上一个刻度上。
fn should_emit_progress(elapsed: std::time::Duration, is_final: bool) -> bool {
    is_final || elapsed >= PROGRESS_INTERVAL
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
    // 路径先解析好：这条分支在动任何轮次状态之前返回，失败时不会留下占位的取消句柄。
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?;
    let dir = cache::cache_dir(&local_root);

    let (pending, version, generation, cancel) = {
        let mut inner = service.lock_inner();
        let pending = inner
            .pending
            .clone()
            .ok_or_else(|| "没有待下载的更新".to_string())?;
        // 清单已经解析过一次，这里重新解析拿纯 semver 版本号：`write_cache` 会用它做文件名，
        // 并要求清单顶层 `version` 与之完全一致。起跑前就定下版本，提交时用它判断待下载
        // 状态是否还是同一份。
        let version = manifest::parse_release(&pending.raw)?.version;
        let (generation, cancel) = begin_download(&mut inner)?;
        (pending, version, generation, cancel)
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
            let is_final = progress
                .total
                .is_some_and(|total| progress.downloaded >= total);
            // 没有上次发送记录时视为间隔早就过了，首块立刻转发。
            let elapsed = last_emit.map_or(PROGRESS_INTERVAL, |last| last.elapsed());
            if should_emit_progress(elapsed, is_final) {
                last_emit = Some(std::time::Instant::now());
                let _ = app_for_progress.emit("update://progress", progress);
            }
        },
    )
    .await;

    let (bytes, source) = match result {
        Ok(value) => value,
        // 失败也要释放本轮句柄，否则“已有下载在进行”会一直挡住用户重试。
        Err(download::DownloadError::Cancelled) => {
            release_download(&mut service.lock_inner(), generation);
            return Err("已取消下载".to_string());
        }
        Err(download::DownloadError::Failed(error)) => {
            release_download(&mut service.lock_inner(), generation);
            return Err(error);
        }
    };

    // 验签只算内存，放在锁外；失败同样要放开本轮句柄。
    let verified = cache::restore_update(&app, pending.raw.clone()).and_then(|update| {
        update
            .verify(&bytes)
            .map_err(|error| format!("更新包验签失败：{error}"))
    });
    if let Err(error) = verified {
        release_download(&mut service.lock_inner(), generation);
        return Err(error);
    }

    // 判定 + 清空 + 写盘在同一个临界区里完成，中间不跨任何 await（`MutexGuard` 不是 Send）。
    {
        let mut inner = service.lock_inner();
        commit_download(
            &mut inner,
            generation,
            &version,
            &pending,
            &source.to_string(),
            &dir,
            &bytes,
        )
    }
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
        assert!(!release_download(&mut inner, first));

        // 取消后可以立刻重开：代号已经递增，不会被“已有下载在进行”挡住。
        let (second, second_cancel) = begin_download(&mut inner).unwrap();
        assert_ne!(second, first);
        // 旧轮次迟到的释放不能清掉新轮次的句柄，也不能获得提交权。
        assert!(!release_download(&mut inner, first));
        assert!(inner.cancel.is_some());
        assert!(!second_cancel.load(Ordering::Relaxed));

        // 新轮次正常收尾：拿到提交权并释放句柄。
        assert!(release_download(&mut inner, second));
        assert!(inner.cancel.is_none());
        // 收尾后可以开下一轮。
        assert!(begin_download(&mut inner).is_ok());
    }

    /// 构造与 `update_check` 一致的最小清单：`commit_download` 会用 `parse_release` 复核版本。
    fn pending_of(version: &str) -> Pending {
        let raw = serde_json::json!({
            "version": version,
            "platforms": {
                "windows-x86_64": {
                    "url": "https://example.com/a.exe",
                    "signature": "sig",
                },
            },
        });
        let asset_url = manifest::parse_release(&raw).unwrap().asset_url;
        Pending { raw, asset_url }
    }

    #[test]
    fn commit_rejects_superseded_generation_without_touching_disk() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache::cache_dir(dir.path());
        let downloaded = pending_of("0.15.0");

        let mut inner = Inner::new();
        let (generation, _cancel) = begin_download(&mut inner).unwrap();
        inner.pending = Some(downloaded.clone());
        // 下载期间被取消：代号已变，迟到的下载即使成功了也不能落盘。
        cancel_download(&mut inner);

        let error = commit_download(
            &mut inner,
            generation,
            "0.15.0",
            &downloaded,
            "https://example.com/a.exe",
            &cache,
            b"abc",
        )
        .unwrap_err();

        assert_eq!(error, "已取消下载");
        assert!(cache::read_cache(&cache).is_none());
        // 待下载状态留给下一轮，不能被这次被拒的提交清掉。
        assert!(inner.pending.is_some());
    }

    #[test]
    fn commit_writes_cache_and_keeps_a_newer_pending() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache::cache_dir(dir.path());
        let downloaded = pending_of("0.15.0");

        let mut inner = Inner::new();
        let (generation, _cancel) = begin_download(&mut inner).unwrap();
        // 提交前一次并发的“检查更新”已经把待下载换成了更新的版本。
        inner.pending = Some(pending_of("0.16.0"));

        commit_download(
            &mut inner,
            generation,
            "0.15.0",
            &downloaded,
            "https://example.com/a.exe",
            &cache,
            b"abc",
        )
        .unwrap();

        let (meta, bytes) = cache::read_cache(&cache).unwrap();
        assert_eq!(meta.version, "0.15.0");
        assert_eq!(bytes, b"abc");
        // 更新的待下载状态不能被这次提交清掉。
        assert!(inner.pending.is_some());
        // 本轮已收尾：句柄释放，下一轮可以立刻开始。
        assert!(inner.cancel.is_none());
        assert!(begin_download(&mut inner).is_ok());
    }

    #[test]
    fn commit_clears_pending_once_the_same_version_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache::cache_dir(dir.path());
        let downloaded = pending_of("0.15.0");

        let mut inner = Inner::new();
        let (generation, _cancel) = begin_download(&mut inner).unwrap();
        inner.pending = Some(downloaded.clone());

        commit_download(
            &mut inner,
            generation,
            "0.15.0",
            &downloaded,
            "https://example.com/a.exe",
            &cache,
            b"abc",
        )
        .unwrap();

        // 包已经落盘：再点“检查更新”应该走缓存的安装路径。
        assert!(inner.pending.is_none());
        assert!(inner.cancel.is_none());

        // 写盘失败也要放开本轮，否则用户无法重试。
        let mut inner = Inner::new();
        let (generation, _cancel) = begin_download(&mut inner).unwrap();
        inner.pending = Some(downloaded.clone());
        let error = commit_download(
            &mut inner,
            generation,
            "0.15.0",
            &downloaded,
            "https://example.com/a.exe",
            &cache,
            b"",
        )
        .unwrap_err();
        assert!(error.contains("更新包为空"), "unexpected error: {error}");
        assert!(inner.cancel.is_none());
        assert!(inner.pending.is_some());
    }

    #[test]
    fn progress_is_throttled_but_final_chunk_always_emits() {
        // 刚发过：跳过，避免每个分块都过 IPC。
        assert!(!should_emit_progress(std::time::Duration::ZERO, false));
        // 未到间隔：仍然跳过。
        assert!(!should_emit_progress(PROGRESS_INTERVAL / 2, false));
        // 到达或超过间隔：放行。
        assert!(should_emit_progress(PROGRESS_INTERVAL, false));
        assert!(should_emit_progress(PROGRESS_INTERVAL * 2, false));
        // 最后一帧无论距上次多久都补发，避免前端停在上一个刻度上。
        assert!(should_emit_progress(std::time::Duration::ZERO, true));
        assert!(should_emit_progress(PROGRESS_INTERVAL / 3, true));
    }
}
