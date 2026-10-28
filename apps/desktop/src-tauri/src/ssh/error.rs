use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshError {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<&'static str>,
}

impl SshError {
    fn new(
        code: &'static str,
        message: impl Into<String>,
        hint: impl Into<String>,
        stage: Option<&'static str>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            hint: hint.into(),
            stage,
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(
            "INVALID_INPUT",
            message,
            "请检查输入后重试。",
            None,
        )
    }

    pub fn host_unreachable(message: impl Into<String>) -> Self {
        Self::new(
            "HOST_UNREACHABLE",
            message,
            "请确认地址、端口、防火墙或 VPN 状态。",
            Some("tcp"),
        )
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(
            "TIMEOUT",
            message,
            "连接或采集超时，请检查网络后重试。",
            Some("tcp"),
        )
    }

    pub fn handshake(message: impl Into<String>) -> Self {
        Self::new(
            "HANDSHAKE_FAILED",
            message,
            "SSH 握手失败，请确认服务器 SSH 服务与算法配置。",
            Some("handshake"),
        )
    }

    pub fn auth_failed() -> Self {
        Self::new(
            "AUTH_FAILED",
            "身份认证失败。",
            "请更新密码或私钥后重试。",
            Some("auth"),
        )
    }

    pub fn host_key_mismatch() -> Self {
        Self::new(
            "HOST_KEY_MISMATCH",
            "主机指纹已变化。",
            "连接已阻断。请核对服务器指纹，清除旧记录后重试。",
            Some("hostKey"),
        )
    }

    pub fn session_not_found() -> Self {
        Self::new(
            "SESSION_NOT_FOUND",
            "SSH 会话不存在。",
            "请重新连接该服务器。",
            None,
        )
    }

    pub fn session_closed() -> Self {
        Self::new(
            "SESSION_CLOSED",
            "SSH 会话已断开。",
            "请重新连接后继续。",
            None,
        )
    }

    pub fn collect(message: impl Into<String>) -> Self {
        Self::new(
            "COLLECT_FAILED",
            message,
            "系统信息采集失败，请稍后重试。",
            Some("collect"),
        )
    }

    pub fn terminal(message: impl Into<String>) -> Self {
        Self::new(
            "TERMINAL_FAILED",
            message,
            "终端通道打开失败，请重试。",
            None,
        )
    }

    pub fn credential(message: impl Into<String>) -> Self {
        Self::new(
            "CREDENTIAL_ERROR",
            message,
            "系统凭据库不可用，请重新输入凭据。",
            None,
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            "INTERNAL",
            message,
            "请重试；若持续出现请重启应用。",
            None,
        )
    }

    pub fn sftp_failed(message: impl Into<String>) -> Self {
        Self::new(
            "SFTP_FAILED",
            message,
            "请确认服务器已启用 SFTP 子系统，然后重试。",
            Some("sftp"),
        )
    }

    pub fn path_invalid(message: impl Into<String>) -> Self {
        Self::new("PATH_INVALID", message, "请检查路径后重试。", None)
    }

    pub fn file_not_found(message: impl Into<String>) -> Self {
        Self::new(
            "FILE_NOT_FOUND",
            message,
            "文件可能已被移动或删除，请刷新目录。",
            None,
        )
    }

    pub fn permission_denied(message: impl Into<String>) -> Self {
        Self::new(
            "PERMISSION_DENIED",
            message,
            "当前登录用户没有读取权限。",
            None,
        )
    }

    pub fn file_too_large(message: impl Into<String>) -> Self {
        Self::new(
            "FILE_TOO_LARGE",
            message,
            "请下载到本地后查看。",
            None,
        )
    }

    pub fn file_binary() -> Self {
        Self::new(
            "FILE_BINARY",
            "该文件不是文本文件。",
            "可以在属性中查看元信息，或下载后打开。",
            None,
        )
    }

    pub fn transfer_failed(message: impl Into<String>) -> Self {
        Self::new(
            "TRANSFER_FAILED",
            message,
            "请重试；目录打包失败时可改用终端手动打包。",
            Some("transfer"),
        )
    }

    pub fn unsupported() -> Self {
        Self::new(
            "UNSUPPORTED",
            "该功能仅桌面版可用。",
            "请在 LightGuard 桌面客户端中使用。",
            None,
        )
    }
}

impl std::fmt::Display for SshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SshError {}

impl From<russh::Error> for SshError {
    fn from(value: russh::Error) -> Self {
        let text = value.to_string();
        if text.contains("Connection reset") || text.contains("Network is unreachable") {
            Self::host_unreachable(text)
        } else {
            Self::handshake(text)
        }
    }
}

impl From<std::io::Error> for SshError {
    fn from(value: std::io::Error) -> Self {
        if value.kind() == std::io::ErrorKind::TimedOut {
            Self::timeout(value.to_string())
        } else {
            Self::internal(value.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_serializes_code_and_hint() {
        let err = SshError::auth_failed();
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "AUTH_FAILED");
        assert_eq!(json["stage"], "auth");
        assert!(!json["hint"].as_str().unwrap().is_empty());
    }

    #[test]
    fn error_never_contains_secret_fields() {
        let err = SshError::internal("unexpected");
        let text = serde_json::to_string(&err).unwrap();
        assert!(!text.contains("secret"));
        assert!(!text.contains("passphrase"));
    }
}
