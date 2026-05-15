//! macOS 浮窗：在全屏应用（如 Cursor / VS Code）所在 Space 内弹出。
//!
//! 关键：app 进入 `.accessory` 模式（无 Dock 图标），结合 `CanJoinAllSpaces` +
//! `FullScreenAuxiliary`，浮窗即可悬浮在任何 app 的全屏 Space 之上；activate 时
//! 也不会把 macOS 切回我们的 Space。

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApp, NSApplicationActivationPolicy, NSColor, NSEvent, NSPopUpMenuWindowLevel, NSScreen,
    NSWindow, NSWindowCollectionBehavior,
};
use objc2_foundation::{NSArray, NSPoint, NSRect};
use std::ffi::c_void;

/// 注意：用 `MoveToActiveSpace` 让浮窗在 orderFront 时跟随到当前活动 Space；
/// 配合 `.accessory` 策略，activate 不会反向把 macOS 切回我们的 Space。
/// 不使用 `CanJoinAllSpaces`，会与 `MoveToActiveSpace` 冲突。
const OVERLAY_COLLECTION: NSWindowCollectionBehavior = NSWindowCollectionBehavior::MoveToActiveSpace
    .union(NSWindowCollectionBehavior::FullScreenAuxiliary)
    .union(NSWindowCollectionBehavior::IgnoresCycle);

/// 把进程设为 `.accessory`：无 Dock 图标 / 菜单栏，浮窗可悬浮在其他 app 全屏 Space 之上，
/// 且 `activate` 不会强制把 macOS 拉回我们的 Space。
pub fn set_accessory_activation_policy() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApp(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

fn with_window(ns_window: *mut c_void, f: impl FnOnce(&NSWindow)) {
    if ns_window.is_null() {
        return;
    }
    // SAFETY: pointer comes from Tauri `ns_window()` for the overlay `NSWindow`.
    let window = unsafe { &*ns_window.cast::<NSWindow>() };
    f(window);
}

pub fn configure(ns_window: *mut c_void) {
    with_window(ns_window, |window| {
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setCollectionBehavior(OVERLAY_COLLECTION);
        window.setLevel(NSPopUpMenuWindowLevel);
        window.setHidesOnDeactivate(false);
        let actual = window.collectionBehavior();
        eprintln!(
            "piggytrans: overlay collectionBehavior set -> raw={:?}, level={}",
            actual.0,
            window.level()
        );
    });
}

/// 与旧版 Swift `OverlayWindowLayout.placeWindowNearMouse` 一致（Cocoa 坐标系）。
pub fn place_near_mouse(ns_window: *mut c_void) {
    with_window(ns_window, |window| {
        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("piggytrans: place_near_mouse skipped — not on main thread");
            return;
        };
        let mouse = NSEvent::mouseLocation();
        let frame = window.frame();
        let size = frame.size;
        let margin = 12.0;

        let screens = NSScreen::screens(mtm);
        let screen = screen_containing_mouse(&screens, mouse).or_else(|| screens.firstObject());

        let Some(screen) = screen else {
            return;
        };

        let vf = screen.visibleFrame();
        let mut origin_x = mouse.x + margin;
        let mut origin_y = mouse.y - size.height - margin;

        if origin_x + size.width > vf.origin.x + vf.size.width - margin {
            origin_x = mouse.x - size.width - margin;
        }
        if origin_x < vf.origin.x + margin {
            origin_x = vf.origin.x + margin;
        }
        if origin_y < vf.origin.y + margin {
            origin_y = mouse.y + margin;
        }
        if origin_y + size.height > vf.origin.y + vf.size.height - margin {
            origin_y = vf.origin.y + vf.size.height - size.height - margin;
        }

        origin_y = origin_y
            .max(vf.origin.y + margin)
            .min(vf.origin.y + vf.size.height - size.height - margin);
        origin_x = origin_x
            .max(vf.origin.x + margin)
            .min(vf.origin.x + vf.size.width - size.width - margin);

        window.setFrameOrigin(NSPoint::new(origin_x, origin_y));
    });
}

pub fn activate_and_order_front(ns_window: *mut c_void) {
    with_window(ns_window, |window| {
        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("piggytrans: activate_and_order_front skipped — not on main thread");
            return;
        };
        configure(ns_window);

        // 顺序与旧版 Swift 完全一致：先 makeKeyAndOrderFront，再 activate。
        // 倒过来会让 macOS 在窗口出现前先切到我们的 Space（即桌面）。
        window.makeKeyAndOrderFront(None);
        #[allow(deprecated)]
        NSApp(mtm).activateIgnoringOtherApps(true);
    });
}

fn screen_containing_mouse(
    screens: &NSArray<NSScreen>,
    mouse: NSPoint,
) -> Option<Retained<NSScreen>> {
    let count = screens.count();
    for i in 0..count {
        let screen = screens.objectAtIndex(i);
        if rect_contains_point(screen.frame(), mouse) {
            return Some(screen);
        }
    }
    None
}

fn rect_contains_point(rect: NSRect, point: NSPoint) -> bool {
    point.x >= rect.origin.x
        && point.x < rect.origin.x + rect.size.width
        && point.y >= rect.origin.y
        && point.y < rect.origin.y + rect.size.height
}
