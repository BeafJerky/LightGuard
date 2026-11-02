use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use reqwest::Url;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug)]
pub enum DownloadError {
    Cancelled,
    Failed(String),
}

pub async fn download_asset<F>(
    client: &reqwest::Client,
    candidates: &[Url],
    max_bytes: u64,
    cancel: Arc<AtomicBool>,
    mut on_progress: F,
) -> Result<(Vec<u8>, Url), DownloadError>
where
    F: FnMut(DownloadProgress),
{
    let mut last_error = String::from("没有可用的下载地址");
    for url in candidates {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        match download_one(client, url, max_bytes, &cancel, &mut on_progress).await {
            Ok(bytes) => return Ok((bytes, url.clone())),
            Err(DownloadError::Cancelled) => return Err(DownloadError::Cancelled),
            Err(DownloadError::Failed(error)) => last_error = error,
        }
    }
    Err(DownloadError::Failed(last_error))
}

async fn download_one<F>(
    client: &reqwest::Client,
    url: &Url,
    max_bytes: u64,
    cancel: &AtomicBool,
    on_progress: &mut F,
) -> Result<Vec<u8>, DownloadError>
where
    F: FnMut(DownloadProgress),
{
    let mut response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| DownloadError::Failed(format!("下载请求失败：{error}")))?;
    if !response.status().is_success() {
        return Err(DownloadError::Failed(format!(
            "下载失败：HTTP {}",
            response.status()
        )));
    }
    let total = response.content_length();
    if total.is_some_and(|size| size > max_bytes) {
        return Err(DownloadError::Failed("更新包超过大小上限".to_string()));
    }

    let mut bytes = Vec::with_capacity(total.unwrap_or(0).min(64 * 1024) as usize);
    loop {
        // 在 await 之前检查：进度回调刚把标志置位时无需再等下一块数据。
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| DownloadError::Failed(format!("读取更新包失败：{error}")))?
        else {
            break;
        };
        if bytes.len() as u64 + chunk.len() as u64 > max_bytes {
            return Err(DownloadError::Failed("更新包超过大小上限".to_string()));
        }
        bytes.extend_from_slice(&chunk);
        on_progress(DownloadProgress {
            downloaded: bytes.len() as u64,
            total,
        });
    }
    // 最后一块之后才点到取消的情况也要算取消，不能把已取消的结果写进缓存。
    if cancel.load(Ordering::Relaxed) {
        return Err(DownloadError::Cancelled);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    // 明确绕过系统代理：开发机上的 Clash 等代理可能拦截 127.0.0.1 请求，让用例不稳定。
    fn test_client() -> reqwest::Client {
        // reqwest 以 rustls-no-provider 编译（与 tauri-plugin-updater 对齐），
        // 生产路径由插件在运行时安装 ring provider；测试里需要自己装一次。
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    fn serve(body: &'static [u8], chunks: usize) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                let size = (body.len() + chunks - 1) / chunks;
                for chunk in body.chunks(size.max(1)) {
                    stream.write_all(chunk).unwrap();
                    stream.flush().unwrap();
                }
            }
        });
        addr
    }

    fn serve_without_length(body: &'static [u8]) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
                let _ = stream.write_all(body);
                let _ = stream.flush();
            }
        });
        addr
    }

    #[tokio::test]
    async fn downloads_and_reports_progress() {
        let addr = serve(b"0123456789", 2);
        let url = reqwest::Url::parse(&format!("http://{addr}/asset")).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut seen = Vec::new();
        let (bytes, source) = download_asset(
            &test_client(),
            std::slice::from_ref(&url),
            1024,
            cancel,
            |progress| seen.push(progress.downloaded),
        )
        .await
        .unwrap();
        assert_eq!(bytes, b"0123456789");
        assert_eq!(source, url);
        assert!(seen.last().copied() == Some(10));
    }

    #[tokio::test]
    async fn falls_back_to_the_next_candidate() {
        let dead = reqwest::Url::parse("http://127.0.0.1:9/dead").unwrap();
        let addr = serve(b"ok", 1);
        let live = reqwest::Url::parse(&format!("http://{addr}/asset")).unwrap();
        let (bytes, source) = download_asset(
            &test_client(),
            &[dead, live.clone()],
            1024,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(bytes, b"ok");
        assert_eq!(source, live);
    }

    #[tokio::test]
    async fn stops_when_cancelled() {
        let addr = serve(b"0123456789", 10);
        let url = reqwest::Url::parse(&format!("http://{addr}/asset")).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let result = download_asset(
            &test_client(),
            std::slice::from_ref(&url),
            1024,
            cancel,
            move |_| flag.store(true, Ordering::Relaxed),
        )
        .await;
        assert!(matches!(result, Err(DownloadError::Cancelled)));
    }

    #[tokio::test]
    async fn rejects_oversized_assets() {
        let addr = serve(b"0123456789", 1);
        let url = reqwest::Url::parse(&format!("http://{addr}/asset")).unwrap();
        let result = download_asset(
            &test_client(),
            std::slice::from_ref(&url),
            4,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .await;
        assert!(matches!(result, Err(DownloadError::Failed(_))));
    }

    #[tokio::test]
    async fn rejects_oversized_streams_without_content_length() {
        let addr = serve_without_length(b"0123456789");
        let url = reqwest::Url::parse(&format!("http://{addr}/asset")).unwrap();
        let result = download_asset(
            &test_client(),
            std::slice::from_ref(&url),
            4,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .await;
        assert!(matches!(result, Err(DownloadError::Failed(_))));
    }
}
