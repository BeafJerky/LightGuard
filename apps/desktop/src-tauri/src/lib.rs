pub mod alerts;
pub mod db;
pub mod guard;
pub mod ssh;

#[cfg(test)]
pub mod test_support;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

// 前端注册好关闭监听后才拦截系统关闭，避免启动早期请求被吞掉。
static CLOSE_BRIDGE_READY: AtomicBool = AtomicBool::new(false);

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
        let _ = window.unminimize();
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

            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                tune_window(&window);
            }
            setup_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if CLOSE_BRIDGE_READY.load(Ordering::SeqCst) {
                    api.prevent_close();
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
            app_quit,
            app_close_ready,
            set_window_border_color,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LightGuard");
}
