pub mod alerts;
pub mod db;
pub mod docker;
pub mod guard;
pub mod ssh;

#[cfg(test)]
pub mod test_support;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tauri_plugin_window_state::{AppHandleExt, StateFlags, WindowExt};

// 前端注册好关闭监听后才拦截系统关闭，避免启动早期请求被吞掉。
static CLOSE_BRIDGE_READY: AtomicBool = AtomicBool::new(false);

// 只持久化几何信息：可见性由启动路径决定，装饰与全屏由配置固定。
fn window_state_flags() -> StateFlags {
    StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED
}

// 开机自启由插件注入该参数，用于区分“用户双击”与“系统拉起”。
fn launched_for_autostart<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--autostart")
}

fn rect_intersects(a: (i32, i32, u32, u32), b: (i32, i32, u32, u32)) -> bool {
    let ax2 = a.0 as i64 + a.2 as i64;
    let ay2 = a.1 as i64 + a.3 as i64;
    let bx2 = b.0 as i64 + b.2 as i64;
    let by2 = b.1 as i64 + b.3 as i64;
    // 必须加括号：`x as i64 < y` 会被解析成泛型参数起始而非比较。
    (a.0 as i64) < bx2 && (b.0 as i64) < ax2 && (a.1 as i64) < by2 && (b.1 as i64) < ay2
}

fn window_rect<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Option<(i32, i32, u32, u32)> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some((position.x, position.y, size.width, size.height))
}

// 保存的位置可能来自已拔掉的显示器；此时把窗口拉回主屏避免“看不见的窗口”。
fn clamp_main_window<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let Some(current) = window_rect(window) else {
        return;
    };
    let monitors: Vec<_> = window
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .map(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            (position.x, position.y, size.width, size.height)
        })
        .collect();
    if monitors.is_empty() || monitors.iter().any(|monitor| rect_intersects(current, *monitor)) {
        return;
    }
    let _ = window.center();
}

#[tauri::command]
fn app_quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn app_close_ready() {
    CLOSE_BRIDGE_READY.store(true, Ordering::SeqCst);
}

fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        // 必须先取消最小化再钳制：最小化窗口的 `outer_position()` 是图标矩形
        // （约 -32000,-32000），钳制会误判为“不在任何显示器上”并调用 `center()`。
        // `center()` → `set_outer_position` 会清掉 MAXIMIZED 标记且不改变 VISIBLE，
        // 立刻触发 `ShowWindow(SW_RESTORE)`，导致“最大化 → 最小化 → 托盘显示”
        // 回来时被还原成居中普通窗口。先 `unminimize()` 拿到真实矩形即可原位恢复。
        let _ = window.unminimize();
        clamp_main_window(&window);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_tray<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "tray-show", "显示主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "tray-quit", "退出 LightGuard", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let mut tray = TrayIconBuilder::with_id("lightguard-tray")
        .tooltip("LightGuard · 轻守卫")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray-show" => show_main_window(app),
            "tray-quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
fn set_window_border_color(window: tauri::WebviewWindow, color: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

        let hex = color.trim_start_matches('#');
        if hex.len() != 6 {
            return Err("invalid color".into());
        }
        let value = u32::from_str_radix(hex, 16).map_err(|_| "invalid color".to_string())?;
        let r = value >> 16 & 0xFF;
        let g = value >> 8 & 0xFF;
        let b = value & 0xFF;
        let colorref: u32 = (b << 16) | (g << 8) | r;
        let hwnd = window.hwnd().map_err(|error| error.to_string())?;
        let hwnd = hwnd.0 as _;
        unsafe {
            DwmSetWindowAttribute(hwnd, 34, &colorref as *const _ as _, 4);
        }
    }
    #[cfg(not(windows))]
    let _ = (window, color);
    Ok(())
}

#[cfg(windows)]
fn tune_window(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWCP_ROUND: u32 = 2;

    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as _;
    let corner = DWMWCP_ROUND;
    let border: u32 = 0x0012_0B07;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as _,
            4,
        );
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as _, 4);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例必须最先注册：否则第二个进程会先建好窗口和托盘再被拦下。
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // 自启拉起的第二实例只做唤醒，不抢用户当前焦点。
            if !launched_for_autostart(argv) {
                show_main_window(app);
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(window_state_flags())
                .build(),
        )
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--autostart"])
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config_dir)?;

            // 两个服务与命令层共用同一个实例：`Arc` 既交给 `manage`，也交给
            // `AppState` 保存，生命周期钩子与命令因此看到同一份状态。
            let db_state = db::DbState::new(&config_dir);
            let database = db_state.handle();
            app.manage(db_state);

            let alerts = alerts::AlertService::new(database, Some(app.handle().clone()));
            let guard = guard::GuardService::new(Some(app.handle().clone()));
            app.manage(alerts.clone());
            app.manage(guard.clone());
            app.manage(ssh::state::AppState::new(config_dir, alerts.clone(), guard));
            // 上一轮遗留的活跃告警在这里结清，并向前端广播一次角标。
            alerts.startup();

            // 主窗口配置为默认隐藏，这里显式恢复上次位置再决定是否露面：
            // 自启拉起时留在托盘，用户双击时钳制到可见显示器并聚焦。
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.restore_state(window_state_flags());
                // 圆角与描边要在窗口露面之前设好；自启路径稍后从托盘打开也保留该外观。
                #[cfg(windows)]
                tune_window(&window);
                if launched_for_autostart(std::env::args()) {
                    let _ = window.hide();
                } else {
                    clamp_main_window(&window);
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            setup_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if CLOSE_BRIDGE_READY.load(Ordering::SeqCst) {
                    api.prevent_close();
                    // 关闭到托盘后窗口不再触发事件，主动落盘以保住最后一次位置。
                    let _ = window.app_handle().save_window_state(window_state_flags());
                    let _ = window.emit("lightguard://close-requested", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            alerts::commands::alerts_list,
            alerts::commands::alerts_ignore,
            alerts::commands::alerts_mute,
            alerts::notify::notify_test,
            ssh::commands::ssh_connect,
            ssh::commands::ssh_reconnect,
            ssh::commands::ssh_snapshot,
            ssh::commands::ssh_disconnect,
            ssh::commands::ssh_session_status,
            ssh::commands::ssh_save_credentials,
            ssh::commands::ssh_credential_status,
            ssh::commands::ssh_forget_credentials,
            ssh::commands::ssh_forget_host_key,
            ssh::commands::ssh_terminal_open,
            ssh::commands::ssh_terminal_write,
            ssh::commands::ssh_terminal_resize,
            ssh::commands::ssh_terminal_close,
            ssh::commands::ssh_reset_local_data,
            ssh::sftp::commands::sftp_home,
            ssh::sftp::commands::sftp_list,
            ssh::sftp::commands::sftp_stat,
            ssh::sftp::commands::sftp_preview_text,
            ssh::sftp::commands::sftp_preview_image,
            ssh::sftp::commands::sftp_download,
            ssh::sftp::commands::sftp_transfer_cancel,
            db::commands::db_load,
            db::commands::db_write,
            guard::commands::permit_status,
            guard::commands::permit_unlock,
            guard::commands::permit_lock,
            guard::commands::write_preview,
            guard::commands::write_execute,
            guard::commands::write_rollback,
            guard::commands::audit_list,
            docker::commands::docker_capabilities,
            docker::commands::docker_overview,
            docker::commands::docker_containers,
            docker::commands::docker_container_detail,
            docker::commands::docker_stats,
            docker::commands::docker_images,
            docker::commands::docker_registry_search,
            docker::commands::docker_registry_tags,
            docker::commands::docker_write_preview,
            docker::commands::docker_write_execute,
            docker::commands::docker_write_cancel,
            docker::commands::docker_sudo_status,
            docker::commands::docker_registry_config,
            docker::commands::docker_system_preview,
            docker::commands::docker_system_execute,
            docker::commands::docker_container_terminal_open,
            app_quit,
            app_close_ready,
            set_window_border_color,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LightGuard");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_autostart_argument() {
        assert!(launched_for_autostart(["LightGuard.exe", "--autostart"]));
        assert!(!launched_for_autostart(["LightGuard.exe"]));
        assert!(!launched_for_autostart(["--autostart=1"]));
    }

    #[test]
    fn rect_intersection_requires_visible_overlap() {
        let monitor = (0, 0, 1920, 1080);
        assert!(rect_intersects((100, 100, 800, 600), monitor));
        assert!(rect_intersects((-100, -100, 800, 600), monitor));
        assert!(rect_intersects((1500, 900, 800, 600), monitor));
        assert!(!rect_intersects((1920, 0, 800, 600), monitor));
        assert!(!rect_intersects((0, 1081, 800, 600), monitor));
        assert!(rect_intersects((1920, 0, 800, 600), (1920, 0, 1920, 1080)));
    }
}
