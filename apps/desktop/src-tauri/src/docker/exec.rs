//! 在既有 SSH 会话上执行后端构造的固定脚本。
//!
//! 与采样器不同，这里同时收集 stdout、stderr、退出码，支持超时、输出上限、
//! 逐行回调和主动取消；Docker 写操作的进度与 sudo 的 stdin 都依赖这些能力。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh::client::Handle;
use russh::ChannelMsg;

use crate::ssh::error::SshError;
use crate::ssh::session::ClientHandler;

pub struct ExecOptions<'a> {
    pub timeout: Duration,
    pub max_bytes: usize,
    pub stdin: Option<&'a [u8]>,
    pub cancel: Option<Arc<AtomicBool>>,
}

pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: Option<u32>,
    pub truncated: bool,
    pub canceled: bool,
}

impl ExecOutput {
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).to_string()
    }
}

pub async fn run(
    handle: &Handle<ClientHandler>,
    script: &str,
    options: ExecOptions<'_>,
    mut on_line: impl FnMut(&str) + Send,
) -> Result<ExecOutput, SshError> {
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|error| SshError::docker_failed(error.to_string()))?;
    channel
        .exec(true, script)
        .await
        .map_err(|error| SshError::docker_failed(error.to_string()))?;

    let (mut read, writer) = channel.split();
    if let Some(stdin) = options.stdin {
        writer
            .data_bytes(stdin.to_vec())
            .await
            .map_err(|error| SshError::docker_failed(error.to_string()))?;
        writer
            .eof()
            .await
            .map_err(|error| SshError::docker_failed(error.to_string()))?;
    }

    let deadline = Instant::now() + options.timeout;
    let mut output = ExecOutput {
        stdout: Vec::new(),
        stderr: Vec::new(),
        code: None,
        truncated: false,
        canceled: false,
    };
    let mut pending = String::new();

    loop {
        if options
            .cancel
            .as_ref()
            .map(|flag| flag.load(Ordering::Relaxed))
            .unwrap_or(false)
        {
            output.canceled = true;
            let _ = writer.close().await;
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = writer.close().await;
            return Err(SshError::timeout("Docker 命令执行超时"));
        }
        let message = match tokio::time::timeout(
            remaining.min(Duration::from_millis(200)),
            read.wait(),
        )
        .await
        {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(_) => continue,
        };
        match message {
            ChannelMsg::Data { data } => {
                push_chunk(
                    &mut output.stdout,
                    &data,
                    options.max_bytes,
                    &mut output.truncated,
                    &mut pending,
                );
                drain_lines(&mut pending, &mut on_line);
            }
            ChannelMsg::ExtendedData { data, ext: 1 } => {
                push_chunk(
                    &mut output.stderr,
                    &data,
                    options.max_bytes,
                    &mut output.truncated,
                    &mut pending,
                );
            }
            ChannelMsg::ExitStatus { exit_status } => output.code = Some(exit_status),
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }

    if !pending.is_empty() {
        on_line(pending.trim_end_matches('\r'));
    }
    Ok(output)
}

pub(crate) fn push_chunk(
    buffer: &mut Vec<u8>,
    data: &[u8],
    max_bytes: usize,
    truncated: &mut bool,
    pending: &mut String,
) {
    let room = max_bytes.saturating_sub(buffer.len());
    let take = data.len().min(room);
    if take < data.len() {
        *truncated = true;
    }
    buffer.extend_from_slice(&data[..take]);
    pending.push_str(&String::from_utf8_lossy(&data[..take]));
}

pub(crate) fn drain_lines(pending: &mut String, on_line: &mut impl FnMut(&str)) {
    let mut cut = false;
    while pending.len() > 400 {
        let line: String = pending.drain(..400).collect();
        cut = true;
        on_line(&line);
    }
    // 超长行被切段后，其余片段不再等待换行：否则 pending 会留下无法回收的尾巴。
    if cut && !pending.is_empty() && !pending.contains('\n') {
        let line: String = std::mem::take(pending);
        on_line(&line);
    }
    while let Some(index) = pending.find('\n') {
        let line: String = pending.drain(..=index).collect();
        let line = line.trim_end_matches(['\r', '\n']);
        if !line.is_empty() {
            on_line(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_are_capped_and_lines_are_emitted() {
        let mut buffer = Vec::new();
        let mut pending = String::new();
        let mut truncated = false;
        let mut lines = Vec::new();
        let mut emit = |line: &str| lines.push(line.to_string());

        push_chunk(&mut buffer, b"one\ntwo\n", 8, &mut truncated, &mut pending);
        drain_lines(&mut pending, &mut emit);
        push_chunk(&mut buffer, b"three\n", 8, &mut truncated, &mut pending);
        drain_lines(&mut pending, &mut emit);

        assert_eq!(buffer, b"one\ntwo\n");
        assert!(truncated);
        assert_eq!(lines, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn a_long_partial_line_is_flushed_in_bounded_pieces() {
        let mut pending = "x".repeat(900);
        let mut lines = Vec::new();
        let mut emit = |line: &str| lines.push(line.len());
        drain_lines(&mut pending, &mut emit);
        assert_eq!(lines, vec![400, 400, 100]);
        assert!(pending.is_empty());
    }
}
