//! macOS 原生标题栏站点切换控件：下拉选择器 + 循环切换按钮 + 设置按钮，
//! 挂在标题栏右侧（红绿灯保留）。支持站点配置变更后动态重建（rebuild）。

use super::{open_settings_window, sites, switch_tab, ActiveTab};
use tauri::Manager;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSButton, NSLayoutAttribute, NSPopUpButton, NSTitlebarAccessoryViewController, NSView,
};
use objc2_foundation::{NSPoint, NSSize, NSString, NSObject as FoundationNSObject, NSObjectProtocol};

/// 供按钮回调读取的 AppHandle（setup 时设置一次）。
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// 永久存活的 target（NSButton 的 target 是弱引用，必须泄漏存活到 app 生命周期）。
/// 用裸指针存储（TabTarget 为 MainThreadOnly，无法直接跨线程 Send/Sync）。
struct TargetPtr(*mut TabTarget);
unsafe impl Send for TargetPtr {}
unsafe impl Sync for TargetPtr {}
static TARGET: std::sync::OnceLock<TargetPtr> = std::sync::OnceLock::new();

/// 控件句柄（raw pointer，仅主线程读写；rebuild 时更新）。
struct Controls {
    popup: *mut AnyObject,
    cycle_btn: *mut AnyObject,
}
// raw pointer 默认不 Send，但此处仅在主线程读写，安全。
unsafe impl Send for Controls {}
unsafe impl Sync for Controls {}
static CONTROLS: std::sync::OnceLock<std::sync::Mutex<Option<Controls>>> = std::sync::OnceLock::new();

// ObjC 侧 target：接收三个控件的 action，映射到站点切换 / 打开设置。
define_class!(
    #[unsafe(super(FoundationNSObject))]
    #[thread_kind = MainThreadOnly]
    struct TabTarget;

    impl TabTarget {
        // 下拉选择器：用户选中某项后立即切换。
        #[unsafe(method(popupChanged:))]
        fn popup_changed(&self, sender: &AnyObject) {
            let idx: isize = unsafe { msg_send![sender, indexOfSelectedItem] };
            if let Some(app) = APP_HANDLE.get() {
                let cfg = app.state::<sites::SiteStore>().snapshot();
                if let Some(site) = cfg.sites.get(idx as usize) {
                    let _ = switch_tab(app, &site.key);
                }
            }
        }

        // 循环切换按钮：按站点顺序切到下一个。
        #[unsafe(method(cycleClicked:))]
        fn cycle_clicked(&self, _sender: &AnyObject) {
            if let Some(app) = APP_HANDLE.get() {
                let cfg = app.state::<sites::SiteStore>().snapshot();
                if cfg.sites.is_empty() {
                    return;
                }
                let active = app.state::<ActiveTab>().0.lock().unwrap().clone();
                if let Some(idx) = cfg.sites.iter().position(|s| s.key == active) {
                    let next = &cfg.sites[(idx + 1) % cfg.sites.len()];
                    let _ = switch_tab(app, &next.key);
                }
            }
        }

        // 设置按钮：打开设置窗口。
        #[unsafe(method(settingsClicked:))]
        fn settings_clicked(&self, _sender: &AnyObject) {
            if let Some(app) = APP_HANDLE.get() {
                let _ = open_settings_window(app);
            }
        }
    }

    unsafe impl NSObjectProtocol for TabTarget {}
);

impl TabTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

/// 首次挂载（RunEvent::Ready 时调用），等价于一次 rebuild。
pub fn setup(window: &tauri::Window) -> tauri::Result<()> {
    rebuild(window)
}

/// 重建标题栏三件套（下拉 + 循环按钮 + 设置按钮）。可重复调用：
/// 先移除旧 accessory VC，再按当前站点配置重建。必须在主线程执行。
pub fn rebuild(window: &tauri::Window) -> tauri::Result<()> {
    let _ = APP_HANDLE.set(window.app_handle().clone());
    let mtm = MainThreadMarker::new().expect("rebuild runs on main thread");

    // 移除旧的 accessory VC（本应用只挂过 1 个；判空防止空数组 remove 抛异常）。
    if let Ok(nswin) = window.ns_window() {
        let nswin = nswin as *mut NSObject;
        if !nswin.is_null() {
            unsafe {
                let arr: *mut AnyObject = msg_send![nswin, titlebarAccessoryViewControllers];
                let count: usize = msg_send![arr, count];
                if count > 0 {
                    let _: () = msg_send![nswin, removeTitlebarAccessoryViewControllerAtIndex: 0];
                }
            }
        }
    }

    let cfg = window.app_handle().state::<sites::SiteStore>().snapshot();
    let active = window.app_handle().state::<ActiveTab>().0.lock().unwrap().clone();
    let active_idx = cfg
        .sites
        .iter()
        .position(|s| s.key == active)
        .unwrap_or(0);
    let active_title = cfg
        .sites
        .get(active_idx)
        .map(|s| s.title.clone())
        .unwrap_or_default();

    let target_ptr = TARGET.get_or_init(|| {
        let target = TabTarget::new(mtm);
        let leaked: &'static mut Retained<TabTarget> = Box::leak(Box::new(target));
        TargetPtr((&**leaked) as *const TabTarget as *mut TabTarget)
    });
    let target_obj = target_ptr.0 as *mut AnyObject;

    // 循环切换按钮：文字 = 当前站点，点击循环切换。放最左。
    let cycle_btn = NSButton::new(mtm);
    cycle_btn.setTitle(&NSString::from_str(&active_title));
    unsafe {
        cycle_btn.setTarget(Some(&*target_obj));
        cycle_btn.setAction(Some(sel!(cycleClicked:)));
    }
    cycle_btn.sizeToFit();

    // 下拉选择器：列出全部站点，当前站点选中。放中间。
    let popup = NSPopUpButton::new(mtm);
    for s in &cfg.sites {
        popup.addItemWithTitle(&NSString::from_str(&s.title));
    }
    popup.selectItemAtIndex(active_idx as isize);
    unsafe {
        popup.setTarget(Some(&*target_obj));
        popup.setAction(Some(sel!(popupChanged:)));
    }
    popup.sizeToFit();

    // 设置按钮（最右，英文）。
    let settings_btn = NSButton::new(mtm);
    settings_btn.setTitle(&NSString::from_str("Settings"));
    unsafe {
        settings_btn.setTarget(Some(&*target_obj));
        settings_btn.setAction(Some(sel!(settingsClicked:)));
    }
    settings_btn.sizeToFit();

    // 统一控件高度并垂直居中，顺序：循环按钮 | 下拉 | 设置，间距一致。
    let ctrl_h = 24.0f64;
    let gap = 4.0f64;
    let container_h = 28.0f64;
    let y = (container_h - ctrl_h) / 2.0;

    let cycle_w = cycle_btn.frame().size.width;
    let popup_w = popup.frame().size.width;
    let settings_w = settings_btn.frame().size.width;
    cycle_btn.setFrameSize(NSSize::new(cycle_w, ctrl_h));
    popup.setFrameSize(NSSize::new(popup_w, ctrl_h));
    settings_btn.setFrameSize(NSSize::new(settings_w, ctrl_h));

    let total_w = cycle_w + gap + popup_w + gap + settings_w;

    let container = NSView::new(mtm);
    container.setFrameSize(NSSize::new(total_w, container_h));
    container.setFrameOrigin(NSPoint::new(0.0, 0.0));

    cycle_btn.setFrameOrigin(NSPoint::new(0.0, y));
    popup.setFrameOrigin(NSPoint::new(cycle_w + gap, y));
    settings_btn.setFrameOrigin(NSPoint::new(cycle_w + gap + popup_w + gap, y));

    container.addSubview(&cycle_btn);
    container.addSubview(&popup);
    container.addSubview(&settings_btn);

    // 保存控件句柄供 update_active 更新（主线程访问）。
    let ctl = Controls {
        popup: (&*popup as *const NSPopUpButton) as *const AnyObject as *mut AnyObject,
        cycle_btn: (&*cycle_btn as *const NSButton) as *const AnyObject as *mut AnyObject,
    };
    *CONTROLS.get_or_init(|| std::sync::Mutex::new(None)).lock().unwrap() = Some(ctl);

    // 挂到标题栏右侧。
    let vc = NSTitlebarAccessoryViewController::new(mtm);
    vc.setView(&container);
    vc.setLayoutAttribute(NSLayoutAttribute::Right);
    if let Ok(nswin) = window.ns_window() {
        let nswin = nswin as *mut NSObject;
        if !nswin.is_null() {
            let _: () = unsafe { msg_send![nswin, addTitlebarAccessoryViewController: &*vc] };
            eprintln!("[chat_app] titlebar rebuilt: {total_w}x28");
        }
    }

    Ok(())
}

/// 切换后同步标题栏显示：更新下拉选中项与循环按钮文字。必须在主线程执行。
pub fn update_active(window: &tauri::Window, key: &str) {
    let cfg = window.app_handle().state::<sites::SiteStore>().snapshot();
    let Some(idx) = cfg.sites.iter().position(|s| s.key == key) else {
        return;
    };
    let title = &cfg.sites[idx].title;
    let Some(lock) = CONTROLS.get() else {
        return;
    };
    let guard = lock.lock().unwrap();
    if let Some(ctl) = guard.as_ref() {
        unsafe {
            let popup: &NSPopUpButton = &*(ctl.popup as *const NSPopUpButton);
            popup.selectItemAtIndex(idx as isize);
            let btn: &NSButton = &*(ctl.cycle_btn as *const NSButton);
            btn.setTitle(&NSString::from_str(title));
            btn.sizeToFit();
            // 保持与其它控件一致的高度并垂直居中。
            let w = btn.frame().size.width;
            btn.setFrameSize(NSSize::new(w, 24.0));
        }
    }
}
