//! 系统托盘 — 左键还原窗口，右键菜单

use tauri::{
    image::Image,
    menu::{Menu, MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

fn restore_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        crate::set_main_webview_visible(app, true);
        let _ = window.set_focus();
    }
}

static QUITTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn quit_app(app: &AppHandle) {
    if QUITTING.swap(true, std::sync::atomic::Ordering::SeqCst) { return; }
    let app=app.clone();
    std::thread::spawn(move || { crate::cleanup_on_exit(&app); app.exit(0); });
}

pub fn quit_app_public(app: &AppHandle) { quit_app(app); }

fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let restore = MenuItemBuilder::with_id("restore", "打开面板").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    MenuBuilder::new(app).item(&restore).item(&quit).build()
}

fn on_menu_event(app: &AppHandle, id: &str) {
    match id {
        "restore" | "show" => restore_main_window(app),
        "quit" => quit_app(app),
        _ => {}
    }
}

/// 配置系统托盘图标和菜单；返回的 TrayIcon 必须由调用方 keep-alive（manage）
pub fn setup_tray(app: &AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let menu = build_tray_menu(app)?;

    // 优先用专用托盘图；失败再退回窗口默认图标
    let icon = Image::from_bytes(include_bytes!("../../icons/tray-icon-compact.png"))
        .or_else(|_| Image::from_bytes(include_bytes!("../../icons/32x32.png")))
        .or_else(|_| {
            app.default_window_icon()
                .cloned()
                .ok_or_else(|| tauri::Error::FailedToReceiveMessage)
        })?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(false)
        .menu(&menu)
        .tooltip("SaloHUb")
        // 左键还原窗口；右键弹出菜单（Windows 默认）
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            on_menu_event(app, event.id().as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                restore_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    log::info!("System tray icon created");
    Ok(tray)
}
