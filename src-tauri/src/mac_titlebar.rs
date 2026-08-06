//! macOS 原生标题栏标签按钮：三个站点切换按钮放在标题栏右侧，
//! 点击时通过 `switch_tab` 切换内容 webview。红绿灯保留，完全原生体验。

use super::{sites, switch_tab};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBezelStyle, NSButton, NSLayoutAttribute, NSStackView, NSTitlebarAccessoryViewController,
};
use objc2_foundation::{NSObject as FoundationNSObject, NSObjectProtocol, NSString};

/// 供按钮回调读取的 AppHandle（setup 时设置一次）。
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

// ObjC 侧 target：接收三个按钮的点击，按 tag 映射到站点后切换。
define_class!(
    #[unsafe(super(FoundationNSObject))]
    #[thread_kind = MainThreadOnly]
    struct TabTarget;

    impl TabTarget {
        #[unsafe(method(tabClicked:))]
        fn tab_clicked(&self, sender: &AnyObject) {
            let tag: isize = unsafe { msg_send![sender, tag] };
            if let Some(site) = sites::sites().get(tag as usize) {
                if let Some(app) = APP_HANDLE.get() {
                    let _ = switch_tab(app, site.key);
                }
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

/// 在标题栏右侧创建三个站点切换按钮，并挂到窗口标题栏。
pub fn setup(app: &tauri::App, window: &tauri::Window) -> tauri::Result<()> {
    let _ = APP_HANDLE.set(app.handle().clone());

    let mtm = MainThreadMarker::new().expect("setup runs on main thread");
    let target = TabTarget::new(mtm);

    // 按钮放进 NSStackView，自动排列
    let stack = NSStackView::new(mtm);
    for (i, site) in sites::sites().iter().enumerate() {
        let button = NSButton::new(mtm);
        button.setTitle(&NSString::from_str(site.title));
        button.setBezelStyle(NSBezelStyle::Toolbar);
        button.setTag(i as isize);
        unsafe {
            button.setTarget(Some(&target));
            button.setAction(Some(sel!(tabClicked:)));
        }
        stack.addArrangedSubview(&button);
    }

    // 标题栏 accessory：view 是按钮栈，靠右显示
    let vc = NSTitlebarAccessoryViewController::new(mtm);
    vc.setView(&stack);
    vc.setLayoutAttribute(NSLayoutAttribute::Right);

    if let Ok(nswin) = window.ns_window() {
        let nswin = nswin as *mut NSObject;
        if !nswin.is_null() {
            let _: () = unsafe { msg_send![nswin, addTitlebarAccessoryViewController: &*vc] };
        }
    }

    Ok(())
}
