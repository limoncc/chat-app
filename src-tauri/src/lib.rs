mod sites;

#[cfg(target_os = "macos")]
mod mac_titlebar;

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::WindowBuilder,
    webview::WebviewBuilder,
    Emitter, Manager, WebviewUrl,
};

#[cfg(target_os = "macos")]
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
#[cfg(target_os = "macos")]
use tauri::window::{Color, Effect, EffectState, EffectsBuilder};
#[cfg(target_os = "macos")]
use tauri::RunEvent;

/// 当前激活的站点 key（由 activate_tab 更新，供主题/缩放按站点处理）。
struct ActiveTab(std::sync::Mutex<String>);

// Injected into every page load. Detects light/dark theme from <html> and <body>,
// then reports it to Rust via Tauri's IPC bridge.
const THEME_DETECT_SCRIPT: &str = r#"
(function(){
    function getTheme(){
        var el;
        // Check <html>
        el=document.documentElement;
        if(el){
            if(el.classList.contains('dark'))return'dark';
            var a=el.getAttribute('data-theme');
            if(a==='dark')return'dark';
            a=el.getAttribute('data-color-scheme');
            if(a==='dark')return'dark';
            a=el.getAttribute('data-mode');
            if(a==='dark')return'dark';
            if(el.hasAttribute('dark'))return'dark';
        }
        // Check <body> (DeepSeek puts "light"/"dark" in body class)
        el=document.body;
        if(el){
            for(var i=0;i<el.classList.length;i++){
                var c=el.classList[i];
                if(c==='dark')return'dark';
                if(c==='light')return'light';
            }
        }
        return'light';
    }
    function report(t){
        // Try both IPC channels — invoke() is preferred, emit() is a fallback
        try{window.__TAURI_INTERNALS__.invoke('report_theme',{theme:t}).catch(function(){})}catch(e){}
        try{window.__TAURI_INTERNALS__.emit('theme-changed',{theme:t})}catch(e){}
    }
    // Observe both html and body for attribute/class changes
    function setup(){
        report(getTheme());
        var cb=function(){report(getTheme())},opts={attributes:true,attributeFilter:['class','data-theme','data-color-scheme','data-mode','style'],subtree:false};
        [document.documentElement,document.body].filter(Boolean).forEach(function(n){
            new MutationObserver(cb).observe(n,opts);
        });
    }
    if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',setup);
    else setup();
    // Backup polling: every 500ms for 10s, covering delayed theme application
    var n=0,i=setInterval(function(){report(getTheme());if(++n>=20)clearInterval(i)},500);
})();
"#;

// 对配置为 dark 的站点注入：强制页面进入暗色模式。
// qianwen 主题由 html 的 data-theme / data-theme-actual / color-scheme-lock 三个属性控制，
// 只设置这三个属性（不碰 class/body），MutationObserver 仅在属性被改回浅色时兜底强制。
const THEME_FORCE_DARK_SCRIPT: &str = r#"
(function(){
    function darken(){
        var el=document.documentElement;
        if(!el)return;
        el.setAttribute('data-theme','dark');
        el.setAttribute('data-theme-actual','dark');
        el.setAttribute('color-scheme-lock','dark');
    }
    darken();
    if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',darken);
    var opts={attributes:true,attributeFilter:['data-theme','data-theme-actual','color-scheme-lock'],subtree:false};
    try{
        new MutationObserver(function(){
            var el=document.documentElement;
            if(el && (el.getAttribute('data-theme')!=='dark'||el.getAttribute('data-theme-actual')!=='dark')){
                darken();
            }
        }).observe(document.documentElement,opts);
    }catch(e){}
})();
"#;

/// Tauri command: called from JS via invoke() to report theme changes.
/// Only the active tab's theme is applied to the window chrome, so a
/// background webview's theme cannot override the visible window.
#[tauri::command]
fn report_theme(window: tauri::Webview, theme: String, state: tauri::State<'_, ActiveTab>) {
    let active = state.0.lock().unwrap().clone();
    if window.label() != active {
        return;
    }
    let win = window.window();
    let w = win.clone();
    let _ = win.run_on_main_thread(move || {
        apply_window_theme(&w, &theme);
    });
}

/// 内容区 webview 的 y 偏移：macOS 用原生标题栏按钮（无 webview 标签栏），
/// 其它平台有 webview 标签栏。
#[cfg(target_os = "macos")]
fn content_offset() -> f64 {
    0.0
}
#[cfg(not(target_os = "macos"))]
fn content_offset() -> f64 {
    sites::TAB_BAR_HEIGHT
}

/// 切换到指定站点：懒加载其 webview 并显隐切换。macOS 原生标题栏按钮与
/// 其它平台的 webview 标签栏共用此逻辑，保证各站会话保持、切换不重新登录。
fn switch_tab(app: &tauri::AppHandle, tab: &str) -> Result<(), String> {
    let site = app
        .state::<sites::SiteStore>()
        .site_by_key(tab)
        .ok_or_else(|| format!("unknown tab: {tab}"))?;
    let window = app.get_window("main").ok_or("main window not found")?;

    // Lazy-load: create the target site's webview on first switch.
    if window.get_webview(tab).is_none() {
        let phys = window.inner_size().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let (w, h) = (phys.width as f64 / scale, phys.height as f64 / scale);
        let (pos, size) = sites::content_bounds(content_offset(), w, h);
        let mut builder = WebviewBuilder::new(
            site.key.clone(),
            WebviewUrl::External(site.url.parse::<tauri::Url>().map_err(|e| e.to_string())?),
        )
        .initialization_script(THEME_DETECT_SCRIPT);
        // 配置为 dark 的站点强制暗色（默认浅色的站点如 qianwen 需要）。
        if site.theme == "dark" {
            builder = builder.initialization_script(THEME_FORCE_DARK_SCRIPT);
        }
        window
            .add_child(builder.zoom_hotkeys_enabled(true), pos, size)
            .map_err(|e| e.to_string())?;
    }

    *app.state::<ActiveTab>().0.lock().unwrap() = tab.to_string();
    let _ = window.set_title(&site.title);
    apply_tab_visibility(&window);
    // 通知标题栏/工具栏刷新"当前站点"显示（下拉 label 与循环按钮文字）。
    let _ = app.emit("active-changed", site.key);
    #[cfg(target_os = "macos")]
    {
        let w = window.clone();
        let w2 = w.clone();
        let key = tab.to_string();
        let _ = w2.run_on_main_thread(move || {
            mac_titlebar::update_active(&w, &key);
        });
    }

    Ok(())
}

/// Tauri command: called from the tab bar (local page, non-macOS platforms)
/// when the user clicks a tab.
#[tauri::command]
fn activate_tab(app: tauri::AppHandle, tab: String) -> Result<(), String> {
    switch_tab(&app, &tab)
}

/// Tauri command: window controls used by the toolbar. Kept for the
/// "double-click the toolbar to maximize/restore" gesture, mirroring the
/// native title bar behaviour.
#[tauri::command]
fn window_control(app: tauri::AppHandle, action: String) -> Result<(), String> {
    let window = app.get_window("main").ok_or("main window not found")?;
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string()),
        "toggle_maximize" => {
            if window.is_maximized().unwrap_or(false) {
                window.unmaximize().map_err(|e| e.to_string())
            } else {
                window.maximize().map_err(|e| e.to_string())
            }
        }
        // 与系统关闭按钮一致：隐藏到托盘而非退出
        "close" => {
            let _ = window.hide();
            Ok(())
        }
        other => Err(format!("unknown window action: {other}")),
    }
}

/// 生成运行时远程站点白名单 capability（JSON 字符串），供 add_capability 动态注入。
fn runtime_sites_capability(cfg: &sites::SitesConfig) -> Result<String, String> {
    let urls: Vec<String> = cfg.sites.iter().map(|s| s.url.clone()).collect();
    serde_json::to_string(&serde_json::json!({
        "identifier": "sites-runtime",
        "description": "runtime remote site urls",
        "local": false,
        "windows": ["main"],
        "remote": { "urls": urls },
        "permissions": ["core:event:default", "core:webview:allow-set-webview-zoom"]
    }))
    .map_err(|e| e.to_string())
}

/// 站点配置变更后的统一通知：补白名单 + 广播 + macOS 重建标题栏。
fn notify_sites_changed(app: &tauri::AppHandle, cfg: &sites::SitesConfig) -> Result<(), String> {
    app.add_capability(runtime_sites_capability(cfg)?)
        .map_err(|e| e.to_string())?;
    app.emit("sites-changed", cfg.clone()).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    if let Some(w) = app.get_window("main") {
        let w2 = w.clone();
        let _ = w2.run_on_main_thread(move || {
            let _ = mac_titlebar::rebuild(&w);
        });
    }
    Ok(())
}

/// Tauri command: 返回当前站点配置（前端工具栏/设置界面使用）。
#[tauri::command]
fn get_sites(state: tauri::State<'_, sites::SiteStore>) -> Result<sites::SitesConfig, String> {
    Ok(state.snapshot())
}

/// Tauri command: 添加站点。
#[tauri::command]
fn add_site(
    app: tauri::AppHandle,
    site: sites::Site,
    state: tauri::State<'_, sites::SiteStore>,
) -> Result<sites::SitesConfig, String> {
    let cfg = state.add_site(site.clone())?;
    // 同 key 的 webview 可能已存在（删除后重加）：导航到新 URL 复用，避免 add_child 重复 label。
    if let Some(w) = app.get_window("main") {
        if let Some(wv) = w.get_webview(&site.key) {
            let _ = wv.navigate(site.url.parse::<tauri::Url>().map_err(|e| e.to_string())?);
        }
    }
    notify_sites_changed(&app, &cfg)?;
    Ok(cfg)
}

/// Tauri command: 更新站点（不允许修改 key）。
#[tauri::command]
fn update_site(
    app: tauri::AppHandle,
    key: String,
    site: sites::Site,
    state: tauri::State<'_, sites::SiteStore>,
) -> Result<sites::SitesConfig, String> {
    let old_url = state.site_by_key(&key).map(|s| s.url);
    let cfg = state.update_site(&key, site)?;
    // URL 变更时，已存在的 webview 立即导航到新地址。
    if let Some(old) = old_url {
        if let Some(new_site) = state.site_by_key(&key) {
            if new_site.url != old {
                if let Some(w) = app.get_window("main") {
                    if let Some(wv) = w.get_webview(&key) {
                        let _ = wv.navigate(new_site.url.parse::<tauri::Url>().map_err(|e| e.to_string())?);
                    }
                }
            }
        }
    }
    notify_sites_changed(&app, &cfg)?;
    Ok(cfg)
}

/// Tauri command: 删除站点；若删除的是当前激活站点则切到新默认。
#[tauri::command]
fn remove_site(
    app: tauri::AppHandle,
    key: String,
    state: tauri::State<'_, sites::SiteStore>,
) -> Result<sites::SitesConfig, String> {
    let cfg = state.remove_site(&key)?;
    let active = app.state::<ActiveTab>().0.lock().unwrap().clone();
    if active == key {
        let _ = switch_tab(&app, &cfg.default_key);
    }
    notify_sites_changed(&app, &cfg)?;
    Ok(cfg)
}

/// Tauri command: 按给定 key 顺序重排站点。
#[tauri::command]
fn reorder_sites(
    app: tauri::AppHandle,
    keys: Vec<String>,
    state: tauri::State<'_, sites::SiteStore>,
) -> Result<sites::SitesConfig, String> {
    let cfg = state.reorder_sites(keys)?;
    notify_sites_changed(&app, &cfg)?;
    Ok(cfg)
}

/// Tauri command: 设置默认站点。
#[tauri::command]
fn set_default(
    app: tauri::AppHandle,
    key: String,
    state: tauri::State<'_, sites::SiteStore>,
) -> Result<sites::SitesConfig, String> {
    let cfg = state.set_default(&key)?;
    notify_sites_changed(&app, &cfg)?;
    Ok(cfg)
}

/// Tauri command: 打开设置窗口（标题栏/工具栏设置按钮、托盘、菜单共用入口）。
#[tauri::command]
fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    open_settings_window(&app)
}

/// 打开设置窗口（已存在则聚焦）。
fn open_settings_window(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    tauri::webview::WebviewWindowBuilder::new(
        app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("设置 - ChatApp")
    .inner_size(720.0, 640.0)
    .min_inner_size(520.0, 400.0)
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Update the macOS window chrome to match the web page's theme.
#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
fn apply_window_theme(window: &tauri::Window, theme: &str) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApp, NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua};

        let mtm = MainThreadMarker::new().expect("must be on main thread");

        match theme {
            "dark" => {
                let _ = window.set_effects(
                    EffectsBuilder::new()
                        .effect(Effect::Sidebar)
                        .state(EffectState::Active)
                        .radius(0.0)
                        .color(Color(0, 0, 0, 255))
                        .build(),
                );
                let appearance = unsafe { NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua) };
                if let Some(ref a) = appearance {
                    NSApp(mtm).setAppearance(Some(a));
                }
            }
            _ => {
                let _ = window.set_effects(
                    EffectsBuilder::new()
                        .effect(Effect::ContentBackground)
                        .state(EffectState::Active)
                        .radius(0.0)
                        .color(Color(255, 255, 255, 255))
                        .build(),
                );
                let appearance = unsafe { NSAppearance::appearanceNamed(NSAppearanceNameAqua) };
                if let Some(ref a) = appearance {
                    NSApp(mtm).setAppearance(Some(a));
                }
            }
        }
    }
}

/// Place every content webview according to the active tab: the active one sits
/// in the content area (below the tabbar), all others are moved off-screen.
/// Tauri's Webview has no `set_visible`, so visibility is achieved by position.
fn apply_tab_visibility(window: &tauri::Window) {
    let active = window
        .app_handle()
        .state::<ActiveTab>()
        .0
        .lock()
        .unwrap()
        .clone();
    let Ok(phys) = window.inner_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let (w, h) = (phys.width as f64 / scale, phys.height as f64 / scale);
    let (pos, size) = sites::content_bounds(content_offset(), w, h);
    let hidden = tauri::LogicalPosition::new(0.0, -10000.0);

    let sites = window.app_handle().state::<sites::SiteStore>().snapshot().sites;
    for site in &sites {
        if let Some(wv) = window.get_webview(&site.key) {
            if site.key == active {
                let _ = wv.set_position(pos);
                let _ = wv.set_size(size);
            } else {
                let _ = wv.set_position(hidden);
            }
        }
    }
}

/// Sync all webviews to the window's current size: the webview tabbar (non-macOS)
/// stays at the top (height = TAB_BAR_HEIGHT), content webviews follow the active tab.
fn relayout(window: &tauri::Window) {
    // macOS 用原生标题栏按钮，无 webview 标签栏。
    #[cfg(not(target_os = "macos"))]
    {
        let Ok(phys) = window.inner_size() else {
            return;
        };
        let scale = window.scale_factor().unwrap_or(1.0);
        let (w, h) = (phys.width as f64 / scale, phys.height as f64 / scale);
        let (tab_pos, tab_size) = sites::tab_bounds(w, h);
        if let Some(wv) = window.get_webview("tabbar") {
            let _ = wv.set_position(tab_pos);
            let _ = wv.set_size(tab_size);
        }
    }

    apply_tab_visibility(window);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Track webview zoom level as percentage (100 = 100%)
    let zoom_level = Arc::new(AtomicU32::new(100));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(ActiveTab(std::sync::Mutex::new(String::new())))
        .invoke_handler(tauri::generate_handler![
            report_theme,
            activate_tab,
            window_control,
            get_sites,
            add_site,
            update_site,
            remove_site,
            reorder_sites,
            set_default,
            open_settings
        ])
        .on_menu_event(move |app, event| {
            match event.id().as_ref() {
                "settings" => {
                    let _ = open_settings_window(app);
                }
                "quit" => app.exit(0),
                _ => {
                    let state = app.state::<ActiveTab>();
                    let active = state.0.lock().unwrap().clone();
                    if let Some(wv) = app.get_webview(&active) {
                        match event.id().as_ref() {
                            "zoom_in" => {
                                let new = (zoom_level.load(Ordering::Relaxed) + 10).min(500);
                                zoom_level.store(new, Ordering::Relaxed);
                                let _ = wv.set_zoom(new as f64 / 100.0);
                            }
                            "zoom_out" => {
                                let new =
                                    (zoom_level.load(Ordering::Relaxed).saturating_sub(10)).max(25);
                                zoom_level.store(new, Ordering::Relaxed);
                                let _ = wv.set_zoom(new as f64 / 100.0);
                            }
                            "zoom_reset" => {
                                zoom_level.store(100, Ordering::Relaxed);
                                let _ = wv.set_zoom(1.0);
                            }
                            _ => {}
                        }
                    }
                }
            }
        })
        .setup(|app| {
            // --- 装载站点配置（读 sites.json，首次/损坏时回退默认并写入）并托管 ---
            let store = sites::SiteStore::load(app.handle())?;
            app.manage(store);
            let default_key = app.state::<sites::SiteStore>().default_key();
            *app.state::<ActiveTab>().0.lock().unwrap() = default_key.clone();
            // 运行时注入远程站点 URL 白名单（替代静态 capability 的 remote.urls）。
            app.add_capability(runtime_sites_capability(
                &app.state::<sites::SiteStore>().snapshot(),
            )?)?;

            // --- Create the main window (no built-in webview; everything is added via add_child) ---
            #[allow(unused_mut)]
            let mut window_builder = WindowBuilder::new(app, "main")
                .title("ChatApp")
                .inner_size(1200.0, 800.0)
                .min_inner_size(400.0, 300.0)
                .resizable(true);
            #[cfg(target_os = "macos")]
            let window_builder = window_builder
                .hidden_title(true)
                .title_bar_style(tauri::TitleBarStyle::Transparent);
            let window = window_builder.build()?;

            let phys = window.inner_size()?;
            let scale = window.scale_factor()?;
            let (win_w, win_h) = (phys.width as f64 / scale, phys.height as f64 / scale);

            // --- Webview tab bar (local page) --- 仅非 macOS；macOS 用原生标题栏按钮。
            #[cfg(not(target_os = "macos"))]
            window.add_child(
                WebviewBuilder::new("tabbar", WebviewUrl::App("index.html".into()))
                    .zoom_hotkeys_enabled(true),
                sites::tab_bounds(win_w, win_h).0,
                sites::tab_bounds(win_w, win_h).1,
            )?;

            // --- Default content webview (lazy: other sites are created on first switch) ---
            let default_site = app
                .state::<sites::SiteStore>()
                .site_by_key(&default_key)
                .ok_or("default site missing")?;
            let mut default_builder = WebviewBuilder::new(
                default_site.key.clone(),
                WebviewUrl::External(default_site.url.parse().expect("default site url")),
            )
            .initialization_script(THEME_DETECT_SCRIPT);
            if default_site.theme == "dark" {
                default_builder = default_builder.initialization_script(THEME_FORCE_DARK_SCRIPT);
            }
            window.add_child(
                default_builder.zoom_hotkeys_enabled(true),
                sites::content_bounds(content_offset(), win_w, win_h).0,
                sites::content_bounds(content_offset(), win_w, win_h).1,
            )?;

            apply_window_theme(&window, &default_site.theme);

            // --- App menu bar (macOS) ---
            #[cfg(target_os = "macos")]
            {
                let zoom_in =
                    MenuItem::with_id(app, "zoom_in", "Zoom In", true, Some("CmdOrCtrl+="))?;
                let zoom_out =
                    MenuItem::with_id(app, "zoom_out", "Zoom Out", true, Some("CmdOrCtrl+-"))?;
                let zoom_reset =
                    MenuItem::with_id(app, "zoom_reset", "Actual Size", true, Some("CmdOrCtrl+0"))?;
                let settings_item =
                    MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;

                let edit_menu = Submenu::with_items(
                    app,
                    "Edit",
                    true,
                    &[
                        &PredefinedMenuItem::undo(app, None::<&str>)?,
                        &PredefinedMenuItem::redo(app, None::<&str>)?,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::cut(app, None::<&str>)?,
                        &PredefinedMenuItem::copy(app, None::<&str>)?,
                        &PredefinedMenuItem::paste(app, None::<&str>)?,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::select_all(app, None::<&str>)?,
                    ],
                )?;

                let view_menu = Submenu::with_items(
                    app,
                    "View",
                    true,
                    &[
                        &zoom_in,
                        &zoom_out,
                        &PredefinedMenuItem::separator(app)?,
                        &zoom_reset,
                    ],
                )?;

                let app_menu = Submenu::with_items(
                    app,
                    "ChatApp",
                    true,
                    &[
                        &PredefinedMenuItem::about(app, Some("About ChatApp"), None)?,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::services(app, None::<&str>)?,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::hide(app, None::<&str>)?,
                        &PredefinedMenuItem::hide_others(app, None::<&str>)?,
                        &PredefinedMenuItem::show_all(app, None::<&str>)?,
                        &PredefinedMenuItem::separator(app)?,
                        &settings_item,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::quit(app, None::<&str>)?,
                    ],
                )?;

                let window_menu = Submenu::with_items(
                    app,
                    "Window",
                    true,
                    &[
                        &PredefinedMenuItem::minimize(app, None::<&str>)?,
                        &PredefinedMenuItem::fullscreen(app, None::<&str>)?,
                        &PredefinedMenuItem::separator(app)?,
                        &PredefinedMenuItem::close_window(app, None::<&str>)?,
                        &PredefinedMenuItem::bring_all_to_front(app, None::<&str>)?,
                    ],
                )?;

                app.set_menu(Menu::with_items(
                    app,
                    &[&app_menu, &edit_menu, &view_menu, &window_menu],
                )?)?;
            }

            // --- System tray ---
            let show = MenuItemBuilder::with_id("show", "Show ChatApp").build(app)?;
            let settings = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show, &settings, &quit])
                .build()?;

            let img = image::load_from_memory(include_bytes!("../icons/icon.png"))
                .expect("failed to load tray icon")
                .to_rgba8();
            let (width, height) = img.dimensions();
            let icon = tauri::image::Image::new_owned(img.into_raw(), width, height);

            TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(w) = app.get_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "settings" => {
                        let _ = open_settings_window(app);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Resized(_) => {
                relayout(window);
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                let _ = window.hide();
                api.prevent_close();
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        // `_` 前缀：非 macOS 下此闭包体为空，避免 unused variable 报错。
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            match _event {
                // App fully launched, window is ready: native titlebar tab
                // buttons must be added here (adding earlier has no effect).
                RunEvent::Ready => {
                    if let Some(w) = _app.get_window("main") {
                        let _ = mac_titlebar::setup(&w);
                    }
                }
                RunEvent::Reopen { .. } => {
                    if let Some(w) = _app.get_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                _ => {}
            }
        });
}
