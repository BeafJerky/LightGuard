use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use russh::client::Msg;
use russh::{ChannelMsg, ChannelReadHalf, ChannelWriteHalf};
use tokio::sync::Mutex;

use super::dto::TerminalEvent;
use super::error::SshError;
use super::session::ClientHandler;

pub const MAX_TERMINALS_PER_SERVER: usize = 8;

pub struct TerminalEntry {
    pub server_id: String,
    pub title: String,
    pub writer: ChannelWriteHalf<Msg>,
}

pub struct TerminalRead {
    pub id: String,
    pub default_title: String,
    pub read: ChannelReadHalf,
}

pub async fn open(
    handle: &russh::client::Handle<ClientHandler>,
    server_id: &str,
    default_title: &str,
    cols: u32,
    rows: u32,
) -> Result<(TerminalEntry, TerminalRead), SshError> {
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|error| SshError::terminal(error.to_string()))?;
    channel
        .request_pty(true, "xterm-256color", cols, rows, 0, 0, &[])
        .await
        .map_err(|error| SshError::terminal(error.to_string()))?;
    channel
        .request_shell(true)
        .await
        .map_err(|error| SshError::terminal(error.to_string()))?;

    let (read, writer) = channel.split();
    let id = uid("t");
    let entry = TerminalEntry {
        server_id: server_id.to_string(),
        title: default_title.to_string(),
        writer,
    };
    let reader = TerminalRead {
        id,
        default_title: default_title.to_string(),
        read,
    };
    Ok((entry, reader))
}

pub fn spawn_reader(
    mut reader: TerminalRead,
    on_event: tauri::ipc::Channel<TerminalEvent>,
    terminals: Arc<Mutex<HashMap<String, TerminalEntry>>>,
) {
    let id = reader.id.clone();
    let default_title = reader.default_title;
    tokio::spawn(async move {
        let mut title = default_title;
        loop {
            match reader.read.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    if on_event
                        .send(TerminalEvent::Data {
                            data: BASE64.encode(&data),
                        })
                        .is_err()
                    {
                        break;
                    }
                    if let Some(next) = extract_title(&data) {
                        if next != title {
                            title = next.clone();
                            let _ = on_event.send(TerminalEvent::Title { title: next });
                        }
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    let _ = on_event.send(TerminalEvent::Exit {
                        code: Some(exit_status as i32),
                    });
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                    let _ = on_event.send(TerminalEvent::Closed {
                        reason: "remote-closed".to_string(),
                    });
                    break;
                }
                Some(_) => {}
            }
        }
        terminals.lock().await.remove(&id);
    });
}

pub async fn write(entry: &TerminalEntry, data: &str) -> Result<(), SshError> {
    entry
        .writer
        .data_bytes(data.as_bytes().to_vec())
        .await
        .map_err(|error| SshError::terminal(error.to_string()))
}

pub async fn resize(entry: &TerminalEntry, cols: u32, rows: u32) -> Result<(), SshError> {
    entry
        .writer
        .window_change(cols, rows, 0, 0)
        .await
        .map_err(|error| SshError::terminal(error.to_string()))
}

pub async fn close(entry: TerminalEntry) {
    let _ = entry.writer.eof().await;
    let _ = entry.writer.close().await;
}

pub fn extract_title(data: &[u8]) -> Option<String> {
    let mut index = 0;
    while index + 3 < data.len() {
        if data[index] == 0x1b && data[index + 1] == b']' {
            let start = index + 2;
            let mut cursor = start;
            while cursor < data.len() {
                let end = if data[cursor] == 0x07 {
                    Some(cursor)
                } else if data[cursor] == 0x1b && data.get(cursor + 1) == Some(&b'\\') {
                    Some(cursor)
                } else {
                    None
                };
                if let Some(end) = end {
                    let body = String::from_utf8_lossy(&data[start..end]);
                    let body = body.trim();
                    if let Some(title) = body
                        .strip_prefix("0;")
                        .or_else(|| body.strip_prefix("2;"))
                    {
                        let title = sanitize_title(title);
                        if !title.is_empty() {
                            return Some(title);
                        }
                    }
                    break;
                }
                cursor += 1;
            }
        }
        index += 1;
    }
    None
}

fn sanitize_title(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect::<String>()
        .trim()
        .to_string()
}

fn uid(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{millis:x}{counter:04x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_osc_title() {
        assert_eq!(
            extract_title(b"\x1b]0;admin@host: ~\x07"),
            Some("admin@host: ~".to_string())
        );
        assert_eq!(
            extract_title(b"\x1b]2;build-01\x1b\\rest"),
            Some("build-01".to_string())
        );
    }

    #[test]
    fn ignores_other_sequences_and_control_chars() {
        assert_eq!(extract_title(b"\x1b[1;32mhello\x1b[0m"), None);
        assert_eq!(
            extract_title(b"\x1b]0;ab\x07cd\x1b]0;xy\x07"),
            Some("ab".to_string())
        );
        assert_eq!(extract_title(b"\x1b]0;\x07"), None);
    }
}
