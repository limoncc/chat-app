//! macOS 原生标题栏标签按钮：三个站点切换按钮放在标题栏右侧，
//! 点击时通过 `switch_tab` 切换内容 webview。红绿灯保留，完全原生体验。

use super::{sites, switch_tab};
use tauri::Manager;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSSegmentedControl, NSSegmentStyle, NSLayoutAttribute, NSView,
    NSTitlebarAccessoryViewController,
};
use objc2_foundation::{NSPoint, NSSize, NSObject as FoundationNSObject, NSObjectProtocol, NSString};

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
            let seg: isize = unsafe { msg_send![sender, selectedSegment] };
            if let Some(site) = sites::sites().get(seg as usize) {
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
/// 必须在窗口就绪后调用（RunEvent::Ready），否则按钮不会显示。
pub fn setup(window: &tauri::Window) -> tauri::Result<()> {
    let _ = APP_HANDLE.set(window.app_handle().clone());

    let mtm = MainThreadMarker::new().expect("setup runs on main thread");
    // NSButton 的 target 是弱引用，必须让 target 永久存活（泄漏到 app 生命周期）。
    let target = TabTarget::new(mtm);
    let target_ref = Box::leak(Box::new(target));

    // 显式 frame 的容器视图 + NSSegmentedControl（macOS 原生分段控件，选中高亮）
    let container = NSView::new(mtm);
    let seg_w = 220.0f64;
    container.setFrameSize(NSSize::new(seg_w, 28.0));
    container.setFrameOrigin(NSPoint::new(0.0, 0.0));

    let control = NSSegmentedControl::new(mtm);
    control.setSegmentCount(sites::sites().len() as isize);
    for (i, site) in sites::sites().iter().enumerate() {
        control.setLabel_forSegment(&NSString::from_str(site.title), i as isize);
    }
    control.setSelectedSegment(0);
    control.setSegmentStyle(NSSegmentStyle::TexturedRounded);
    control.setFrameSize(NSSize::new(seg_w, 24.0));
    control.setFrameOrigin(NSPoint::new(0.0, 2.0));
    unsafe {
        control.setTarget(Some(&**target_ref));
        control.setAction(Some(sel!(tabClicked:)));
    }
    container.addSubview(&control);

    // 标题栏 accessory：view 是按钮容器，靠右显示
    let vc = NSTitlebarAccessoryViewController::new(mtm);
    vc.setView(&container);
    vc.setLayoutAttribute(NSLayoutAttribute::Right);

    if let Ok(nswin) = window.ns_window() {
        let nswin = nswin as *mut NSObject;
        if !nswin.is_null() {
            let _: () = unsafe { msg_send![nswin, addTitlebarAccessoryViewController: &*vc] };
            // 诊断：确认 accessory 已挂载、窗口样式与按钮可见性
            let count: usize = unsafe {
                let arr: *mut objc2::runtime::NSObject =
                    msg_send![nswin, titlebarAccessoryViewControllers];
                msg_send![arr, count]
            };
            let style: u64 = unsafe { msg_send![nswin, styleMask] };
            let hidden: bool = unsafe { msg_send![&*vc, isHidden] };
            eprintln!(
                "[chat_app] accessory={count} styleMask={style:#x} vcHidden={hidden} containerFrame={seg_w}x28"
            );
        }
    }

    Ok(())
}
