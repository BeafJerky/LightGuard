# LightGuard 轻守卫

面向个人与小型团队的服务器值守桌面客户端：真实 SSH、类 Windows 的服务器工作区、只读采集、文件管理、告警值守与写操作护栏。

当前版本 **0.13.0**，Tauri 2 + Vue 3，Windows x64。

## 它是什么

- 桌面客户端通过 russh 直连服务器，工作区是 LightGuard 前端渲染的「类 Windows 服务器桌面」：图标、任务栏、开始菜单、可缩放窗口，窗口内容全部来自真实只读快照。
- 终端是真实 SSH PTY，命令由用户自己输入，产品不代为执行。
- 浏览器形态只做界面预览，不发起 SSH 或任何后端网络请求。
- 它不是远程图形桌面、不是 Web 运维面板，也不是批量自动化平台。

## 主要能力

| 模块 | 状态 |
| --- | --- |
| 真实 SSH | 密码 / 私钥（含口令）、TOFU 主机指纹、多标签终端、断线重连 |
| 只读采集 | 系统信息、性能、磁盘、网络、进程、连接信息；固定只读脚本 |
| 服务器桌面 | 图标 / 任务栏 / 开始菜单 / 多窗口、悬浮与通栏状态条、托盘常驻 |
| 文件管理（0.11.0） | 复用同一 SSH 会话的 SFTP：目录树、三视图、文本 / 图片预览、单文件与目录 tar.gz 下载 |
| 值守告警（0.12.0） | 10 秒轻量采样；CPU / 内存 / 磁盘 / 负载 / 离线阈值、主机指纹变更、登录失败与来源变化；告警中心、静音与忽略、原生通知、托盘角标 `99+` |
| 运维许可（0.12.0） | 默认锁定、逐台限时解锁（5 / 15 / 30 分钟）、到期自动上锁、连接条 / 窗口边框 / 任务栏三处可见 |
| 写操作护栏（0.12.0） | 影响面预览、60 秒内一次性确认、同机写锁串行、只追加审计、回退点 |
| Docker（0.13.0） | 服务总览与启停、容器启停 / 重启 / 删除、容器终端、本地镜像、镜像仓库搜索与拉取（标签可下拉选版本）、按名字查版本、加速源与代理设置；全部写操作复用同一许可与护栏 |
| 主题 | 7 套风格主题 × 控制台 / 服务器桌面双作用域独立切换 |

## 技术栈

- 前端：Vue 3 + TypeScript + Vite；内部组件库 `@lightguard/lightui`；naive-ui、xterm.js、motion-v。
- 后端：Rust + Tauri 2；`russh`（SSH / SFTP）、`rusqlite`（SQLite，bundled，WAL）、keyring + AES-256-GCM（凭据）。
- 工程：pnpm workspace、Vitest、`cargo test`。

## 目录结构

```text
apps/desktop/          桌面应用：Vue 前端 + src-tauri（Rust 后端与 Tauri 命令）
packages/lightui/      内部 UI 组件库、主题 token 与样式
tools/                 截图、CDP 驱动、主题 / 对比度 / 无障碍校验脚本
```

## 环境要求

- Windows 10 / 11 x64（当前唯一验证平台）与 WebView2 运行时
- Node.js ≥ 20 与 pnpm 11
- Rust ≥ 1.77.2 与 MSVC 工具链
- 构建安装包由 Tauri 自动准备 NSIS

## 快速开始

```bash
pnpm install

# 浏览器形态：只预览界面，不连接服务器
pnpm dev

# 桌面形态：真实 SSH
pnpm --filter @lightguard/desktop desktop:dev
```

## 构建与测试

```bash
pnpm build                                      # 类型检查 + 前端产物
pnpm typecheck
pnpm --filter @lightguard/desktop test          # 前端单测
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib
pnpm --filter @lightguard/desktop desktop:build # NSIS 安装包
```

安装包输出在 `apps/desktop/src-tauri/target/release/bundle/nsis/`。

## 安全模型

- 凭据保存在系统钥匙串主密钥加密的密文中，配置备份不包含凭据；任何凭据都不写入日志。
- 主机密钥采用 TOFU：首次连接记录指纹，后续变更会阻断连接并产生告警。
- 采集脚本为常量，不拼接用户输入。
- 产品发起的写操作默认全部锁定：需要该服务器的有效许可、影响面预览与一次性确认；远端只调用 SFTP `create_dir` / `remove_dir` 与固定的 docker 子命令，不拼接 shell 字符串，不提供批量或跨服务器入口；同机同一时刻只有一个写操作；审计只追加。
- 许可只约束产品发起的写操作，不约束终端命令。

## 已知限制

- 仅在 Windows 上验证；Web 形态只做界面预览。
- 产品发起的写操作为「新建文件夹」与 Docker 相关操作；文件上传 / 重命名 / 删除 / 改权限尚未提供。
- Docker 管理需要目标服务器具备可用的 `docker` 命令与 sudo 权限；镜像搜索由 `docker search` 直连 Docker Hub，加速源只作用于拉取与标签查询；Compose 与网络页面后续开放。
- 应用完全退出期间不采样、不告警；系统通知不支持点击跳转。
- 登录监控依赖可读的 `journalctl` 或 `auth.log`，不可读时该规则不可用。
- 暂无代码签名与自动更新。

## 文档与路线图

- 需求、设计与验证记录维护在本仓库之外，不随仓库发布。
- 后续版本：软件仓库，以及 Docker 的 Compose 与网络能力，继续复用现有的许可与护栏。
