//! System tray icon: closing the window hides it instead of quitting, so
//! playback keeps running in the background. Left-click / "Show YouTube"
//! restores the window; "Quit" actually exits.
//!
//! The menu also carries the app's user-facing toggles (theme, ad block,
//! analytics, cinema, always-on-top, close-to-tray) as check items. Their
//! initial checked state comes from the loaded `Settings`; the live ones
//! (always-on-top, close-to-tray) take effect immediately via event-loop
//! callbacks, the init-script ones (theme/ads/analytics/cinema) are picked up
//! on the next launch via "Restart".

use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use crate::icon;
use crate::settings::Settings;

pub struct TrayHandle {
    pub _icon: TrayIcon, // kept alive: dropping it removes the tray icon
    pub show_item_id: tray_icon::menu::MenuId,
    pub quit_item_id: tray_icon::menu::MenuId,
    pub restart_item_id: tray_icon::menu::MenuId,
    /// Opens the in-app settings page (youtube-glass custom protocol).
    pub settings_item_id: tray_icon::menu::MenuId,
    /// Live toggle: mirrors its own checked state onto set_always_on_top.
    pub always_on_top_item: CheckMenuItem,
    /// Live toggle: controls whether CloseRequested hides or quits.
    pub close_to_tray_item: CheckMenuItem,
    /// Apply-on-restart toggles (theme / ad block / analytics / cinema). Their
    /// checked state is read back into Settings on click; the effect lands next
    /// launch.
    pub theme_item: CheckMenuItem,
    pub block_ads_item: CheckMenuItem,
    pub analytics_item: CheckMenuItem,
    pub cinema_item: CheckMenuItem,
}

/// Wires the two event channels tray-icon exposes into the tao event loop via
/// a user-event proxy, so the loop wakes up on tray/menu activity instead of
/// needing a polling timer.
///
/// `settings` seeds the initial checked state of every toggle.
///
/// Returns `None` if the tray icon couldn't be created (e.g. `explorer.exe`
/// not running, or the notification area is unavailable). Callers must treat
/// that as "no tray": in particular, closing the window should quit the app
/// rather than hide it - otherwise there'd be no way to bring it back.
pub fn init<T>(
    proxy: tao::event_loop::EventLoopProxy<T>,
    settings: &Settings,
) -> Option<TrayHandle>
where
    T: From<TrayIconEvent> + From<MenuEvent> + Send + 'static,
{
    let tray_proxy = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = tray_proxy.send_event(T::from(event));
    }));

    let menu_proxy = proxy;
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(T::from(event));
    }));

    let menu = Menu::new();
    let show_item = MenuItem::new("Показать YouTube", true, None);
    let settings_item = MenuItem::new("Настройки", true, None);
    let theme_item = CheckMenuItem::new("Тема Liquid Glass", true, settings.theme, None);
    let block_ads_item =
        CheckMenuItem::new("Скрывать рекламные баннеры", true, settings.block_ads, None);
    let analytics_item =
        CheckMenuItem::new("Аналитика видео", true, settings.analytics, None);
    let cinema_item = CheckMenuItem::new("Режим кинотеатра", true, settings.cinema, None);
    let always_on_top_item =
        CheckMenuItem::new("Поверх всех окон", true, settings.always_on_top, None);
    let close_to_tray_item =
        CheckMenuItem::new("Сворачивать в трей при закрытии", true, settings.close_to_tray, None);
    let restart_item = MenuItem::new("Перезапустить", true, None);
    let quit_item = MenuItem::new("Выход", true, None);
    menu.append_items(&[
        &show_item,
        &settings_item,
        &PredefinedMenuItem::separator(),
        &theme_item,
        &block_ads_item,
        &analytics_item,
        &cinema_item,
        &always_on_top_item,
        &close_to_tray_item,
        &PredefinedMenuItem::separator(),
        &restart_item,
        &quit_item,
    ])
    .ok();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("YouTube")
        .with_icon(icon::youtube_tray_icon(32))
        .build();

    let tray = match tray {
        Ok(t) => t,
        Err(e) => {
            crate::logging::log(format!("Tray icon unavailable, disabling minimize-to-tray: {e}"));
            return None;
        }
    };

    Some(TrayHandle {
        _icon: tray,
        show_item_id: show_item.id().clone(),
        quit_item_id: quit_item.id().clone(),
        restart_item_id: restart_item.id().clone(),
        settings_item_id: settings_item.id().clone(),
        always_on_top_item,
        close_to_tray_item,
        theme_item,
        block_ads_item,
        analytics_item,
        cinema_item,
    })
}
