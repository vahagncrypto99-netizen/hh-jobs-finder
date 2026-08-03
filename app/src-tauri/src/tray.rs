//! Tray icon (menu-bar entry) and popover show/hide plumbing.
//! Adapted from naeasy: the window is a Spotlight-style NSPanel anchored
//! under the tray icon, the app itself never appears in the Dock.

use tauri::Manager;

/// Make every normal-level window join all Spaces and fullscreen Spaces, so
/// macOS never switches Space to "follow" the app.
#[cfg(target_os = "macos")]
pub fn make_windows_join_all_spaces(app: &tauri::AppHandle) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSWindow, NSWindowCollectionBehavior};

    const BEHAVIOR: NSWindowCollectionBehavior = NSWindowCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces.0
            | NSWindowCollectionBehavior::FullScreenAuxiliary.0,
    );

    if let Some(w) = app.get_webview_window("main") {
        if let Ok(ptr) = w.ns_window() {
            let ns_window = unsafe { &*(ptr as *const NSWindow) };
            ns_window.setCollectionBehavior(BEHAVIOR);
        }
    }
    if let Some(mtm) = MainThreadMarker::new() {
        let ns_app = NSApplication::sharedApplication(mtm);
        for w in ns_app.windows().iter() {
            if w.level() == 0 {
                w.setCollectionBehavior(BEHAVIOR);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn make_windows_join_all_spaces(_app: &tauri::AppHandle) {}

fn position_main_under_tray(app: &tauri::AppHandle) {
    if let (Some(tray), Some(window)) = (app.tray_by_id("main-tray"), app.get_webview_window("main"))
    {
        if let Ok(Some(rect)) = tray.rect() {
            position_under_tray(&window, &rect);
        }
    }
}

/// Hide the popover — the single hide path for every trigger.
pub fn hide_main(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt as _;
        if let Ok(panel) = app.get_webview_panel("main") {
            panel.order_out(None);
            return;
        }
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

/// Show, position and focus the popover.
pub fn show_main(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt as _;
        if let Ok(panel) = app.get_webview_panel("main") {
            make_windows_join_all_spaces(app);
            position_main_under_tray(app);
            panel.show();
            return;
        }
    }
    if let Some(w) = app.get_webview_window("main") {
        make_windows_join_all_spaces(app);
        position_main_under_tray(app);
        let _ = w.set_always_on_top(true);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn toggle_main(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt as _;
        if let Ok(panel) = app.get_webview_panel("main") {
            if panel.is_visible() {
                hide_main(app);
            } else {
                show_main(app);
            }
            return;
        }
    }
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) {
            hide_main(app);
        } else {
            show_main(app);
        }
    }
}

/// Build the menu-bar entry.
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItemBuilder::with_id("show", "Открыть hh-jobs").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Выйти").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;

    let tray_builder = TrayIconBuilder::with_id("main-tray");
    let tray_builder = match tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
        Ok(img) => tray_builder.icon(img).icon_as_template(true),
        Err(_) => tray_builder.icon(app.default_window_icon().unwrap().clone()),
    };

    tray_builder
        .tooltip("hh-jobs — авто-отклики hh.ru")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "show" => show_main(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Center the popover under the tray icon, clamped to the monitor.
fn position_under_tray(window: &tauri::WebviewWindow, rect: &tauri::Rect) {
    use tauri::{PhysicalPosition, Position};

    let win_w = match window.outer_size() {
        Ok(s) => s.width as f64,
        Err(_) => return,
    };
    let sf = window.scale_factor().unwrap_or(1.0);

    let tray_x = match rect.position {
        Position::Physical(p) => p.x as f64,
        Position::Logical(p) => p.x * sf,
    };
    let tray_w = match rect.size {
        tauri::Size::Physical(s) => s.width as f64,
        tauri::Size::Logical(s) => s.width * sf,
    };

    let (mon_x, mon_y, mon_w) = match window.primary_monitor() {
        Ok(Some(m)) => {
            let p = m.position();
            let s = m.size();
            (p.x as f64, p.y as f64, s.width as f64)
        }
        _ => {
            let _ = window.center();
            return;
        }
    };

    let mut x = tray_x + tray_w / 2.0 - win_w / 2.0;
    let min_x = mon_x + 8.0;
    let max_x = mon_x + mon_w - win_w - 8.0;
    if x < min_x {
        x = min_x;
    }
    if max_x > min_x && x > max_x {
        x = max_x;
    }
    let y = mon_y + (26.0 * sf);

    let _ = window.set_position(Position::Physical(PhysicalPosition {
        x: x as i32,
        y: y as i32,
    }));
}
