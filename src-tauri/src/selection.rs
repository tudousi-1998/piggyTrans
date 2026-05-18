use std::thread;
use std::time::Duration;

#[cfg(not(target_os = "macos"))]
use arboard::Clipboard;
#[cfg(not(target_os = "macos"))]
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

const MAX_CHARS: usize = 3000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionOutcome {
    Text(String),
    PermissionDenied,
    Empty,
}

/// 仅将 CRLF 规范为 LF；单独的 `\r` 不当作换行（直接去掉）。
pub fn normalize_line_endings(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "")
}

pub fn normalize_text(s: &str) -> String {
    normalize_text_preserve_lines(s)
}

fn normalize_text_preserve_lines(s: &str) -> String {
    let t = normalize_line_endings(s).trim().to_string();
    if t.chars().count() > MAX_CHARS {
        t.chars().take(MAX_CHARS).collect()
    } else {
        t
    }
}

fn pick_longer_multiline(preferred: &str, other: &str) -> String {
    let p_has_nl = preferred.contains('\n');
    let o_has_nl = other.contains('\n');
    let p_len = preferred.chars().count();
    let o_len = other.chars().count();
    if p_has_nl && !o_has_nl {
        return preferred.to_string();
    }
    if p_len > o_len {
        return preferred.to_string();
    }
    if p_len == o_len && preferred != other {
        return preferred.to_string();
    }
    if o_len > p_len {
        return other.to_string();
    }
    preferred.to_string()
}

#[cfg(target_os = "macos")]
pub fn fetch_selection() -> SelectionOutcome {
    if !macos_ax::is_process_trusted() {
        return SelectionOutcome::PermissionDenied;
    }

    let ax_text = macos_ax::selected_text_via_accessibility();
    let ax_has_text = ax_text
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty());
    let ax_suggests_selection =
        ax_has_text || macos_ax::focused_has_nonempty_selection_range();

    if !ax_suggests_selection {
        // 无选区：跳过 AXCopy + 长轮询，仅快速探测剪贴板（约 150ms）
        let copy_text = simulate_copy_macos_quick();
        return pick_best_selection(None, copy_text);
    }

    let copy_text = simulate_copy_macos();
    pick_best_selection(ax_text, copy_text)
}

#[cfg(target_os = "windows")]
pub fn fetch_selection() -> SelectionOutcome {
    let copy_text = simulate_copy(CopyModifier::Ctrl);
    pick_best_selection(None, copy_text)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn fetch_selection() -> SelectionOutcome {
    SelectionOutcome::Empty
}

/// 合并 AX 与模拟复制结果。浏览器/网页多行选区以剪贴板为准。
fn pick_best_selection(ax: Option<String>, copy: Option<String>) -> SelectionOutcome {
    let ax_n = ax
        .map(|t| normalize_text_preserve_lines(&t))
        .filter(|s| !s.is_empty());
    let copy_n = copy
        .map(|t| normalize_text_preserve_lines(&t))
        .filter(|s| !s.is_empty());

    // 剪贴板已含换行时直接采用（浏览器 Cmd+C / AXCopy 的可靠信号）
    if let Some(c) = &copy_n {
        if c.contains('\n') {
            return SelectionOutcome::Text(c.clone());
        }
    }

    // AX 只有第一行，但剪贴板更长（常见于浏览器）
    if let (Some(a), Some(c)) = (&ax_n, &copy_n) {
        if !a.contains('\n') && c.chars().count() > a.chars().count() {
            return SelectionOutcome::Text(c.clone());
        }
    }

    let best = match (&ax_n, &copy_n) {
        (Some(a), Some(c)) => pick_longer_multiline(c, a),
        (Some(a), None) => a.clone(),
        (None, Some(c)) => c.clone(),
        (None, None) => String::new(),
    };

    if best.is_empty() {
        SelectionOutcome::Empty
    } else {
        SelectionOutcome::Text(best)
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum CopyModifier {
    Meta,
    Ctrl,
}

#[cfg(target_os = "macos")]
fn simulate_copy_macos_quick() -> Option<String> {
    simulate_copy_macos_impl(CopyStrategy::Quick)
}

#[cfg(target_os = "macos")]
fn simulate_copy_macos() -> Option<String> {
    simulate_copy_macos_impl(CopyStrategy::Full)
}

#[cfg(target_os = "macos")]
enum CopyStrategy {
    Quick,
    Full,
}

#[cfg(target_os = "macos")]
fn simulate_copy_macos_impl(strategy: CopyStrategy) -> Option<String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSRunningApplication, NSWorkspace};

    const KEY_C: u16 = 0x08; // kVK_ANSI_C

    let pasteboard = NSPasteboard::generalPasteboard();
    let before_change_count = pasteboard.changeCount();
    // SAFETY: `NSPasteboardTypeString` is a process-lifetime constant from AppKit.
    let before_string =
        unsafe { pasteboard.stringForType(NSPasteboardTypeString) };
    let before_ns = before_string.as_ref().map(|s| &**s);

    if matches!(strategy, CopyStrategy::Full) {
        // 浏览器网页：AXCopy 往往比模拟按键更稳
        if macos_ax::perform_ax_copy_on_focused() {
            if let Some(text) = poll_pasteboard_after_copy(
                &pasteboard,
                before_change_count,
                before_ns,
                FULL_COPY_POLL_MS,
            ) {
                restore_pasteboard(&pasteboard, before_ns);
                return Some(text);
            }
        }
    }

    let target_pid = macos_ax::focused_app_pid().or_else(|| {
        let self_pid = NSRunningApplication::currentApplication().processIdentifier();
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier())
            .filter(|pid| *pid != self_pid)
    });

    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .or_else(|_| CGEventSource::new(CGEventSourceStateID::HIDSystemState))
        .ok()?;
    let key_down = CGEvent::new_keyboard_event(source.clone(), KEY_C, true).ok()?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    let key_up = CGEvent::new_keyboard_event(source, KEY_C, false).ok()?;
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);

    let post_keys = |use_pid: Option<i32>| {
        if let Some(pid) = use_pid {
            key_down.post_to_pid(pid);
            thread::sleep(Duration::from_millis(20));
            key_up.post_to_pid(pid);
        } else {
            key_down.post(CGEventTapLocation::HID);
            thread::sleep(Duration::from_millis(20));
            key_up.post(CGEventTapLocation::HID);
        }
    };

    let poll_ms = match strategy {
        CopyStrategy::Quick => QUICK_COPY_POLL_MS,
        CopyStrategy::Full => FULL_COPY_POLL_MS,
    };

    post_keys(target_pid);
    let mut captured = poll_pasteboard_after_copy(&pasteboard, before_change_count, before_ns, poll_ms);

    // 部分浏览器对 PostToPid 无响应，再试 HID 投递（仅完整模式）
    if captured.is_none() && matches!(strategy, CopyStrategy::Full) {
        post_keys(None);
        captured = poll_pasteboard_after_copy(&pasteboard, before_change_count, before_ns, poll_ms);
    }

    let captured = captured?;

    restore_pasteboard(&pasteboard, before_ns);

    Some(captured)
}

#[cfg(target_os = "macos")]
fn restore_pasteboard(
    pasteboard: &objc2_app_kit::NSPasteboard,
    before_string: Option<&objc2_foundation::NSString>,
) {
    use objc2_app_kit::NSPasteboardTypeString;
    if let Some(before) = before_string {
        pasteboard.clearContents();
        let _ = unsafe { pasteboard.setString_forType(before, NSPasteboardTypeString) };
    }
}

#[cfg(target_os = "macos")]
const QUICK_COPY_POLL_MS: &[u64] = &[50, 100];
#[cfg(target_os = "macos")]
const FULL_COPY_POLL_MS: &[u64] = &[60, 100, 160, 240, 360, 480];

#[cfg(target_os = "macos")]
fn poll_pasteboard_after_copy(
    pasteboard: &objc2_app_kit::NSPasteboard,
    before_change_count: isize,
    before_string: Option<&objc2_foundation::NSString>,
    delays_ms: &[u64],
) -> Option<String> {
    for &delay_ms in delays_ms {
        thread::sleep(Duration::from_millis(delay_ms));
        if let Some(text) = read_pasteboard_if_changed(pasteboard, before_change_count, before_string)
        {
            return Some(text);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn read_pasteboard_if_changed(
    pasteboard: &objc2_app_kit::NSPasteboard,
    before_change_count: isize,
    before_string: Option<&objc2_foundation::NSString>,
) -> Option<String> {
    use objc2_app_kit::NSPasteboardTypeString;

    let after_change_count = pasteboard.changeCount();
    let after_string = unsafe { pasteboard.stringForType(NSPasteboardTypeString) }
        .map(|s| s.to_string())?;
    let after_norm = normalize_text_preserve_lines(&after_string);
    if after_norm.is_empty() {
        return None;
    }

    let before_norm = before_string
        .map(|s| s.to_string())
        .map(|s| normalize_text_preserve_lines(&s));
    let changed = after_change_count != before_change_count || before_norm.as_deref() != Some(&after_norm);
    if changed {
        Some(after_norm)
    } else {
        None
    }
}

#[cfg(not(target_os = "macos"))]
fn simulate_copy(modifier: CopyModifier) -> Option<String> {
    let mut clipboard = Clipboard::new().ok()?;
    let before = clipboard.get_text().ok();
    let mut enigo = Enigo::new(&Settings::default()).ok()?;

    match modifier {
        CopyModifier::Meta => {
            enigo.key(Key::Meta, Direction::Press).ok()?;
            enigo.key(Key::Unicode('c'), Direction::Click).ok()?;
            enigo.key(Key::Meta, Direction::Release).ok()?;
        }
        CopyModifier::Ctrl => {
            enigo.key(Key::Control, Direction::Press).ok()?;
            enigo.key(Key::Unicode('c'), Direction::Click).ok()?;
            enigo.key(Key::Control, Direction::Release).ok()?;
        }
    }

    thread::sleep(Duration::from_millis(180));
    let after_raw = clipboard.get_text().ok()?;
    let after = after_raw.trim();
    if after.is_empty() {
        return None;
    }
    let before_trim = before.as_ref().map(|b| b.trim().to_string());
    let changed = match &before_trim {
        Some(b) if b == after => false,
        Some(_) => true,
        None => true,
    };
    if !changed {
        return None;
    }
    if let Some(b) = &before {
        let _ = clipboard.set_text(b.clone());
    }
    Some(after.to_string())
}

#[cfg(target_os = "macos")]
mod macos_ax {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    use std::os::raw::c_void;

    type AXError = i32;
    const AX_K_SUCCESS: AXError = 0;
    type AXUIElementRef = *const c_void;
    type CFTypeRef = *const c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> u8;
        fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> u8;
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: core_foundation::string::CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementPerformAction(
            element: AXUIElementRef,
            action: core_foundation::string::CFStringRef,
        ) -> AXError;
        fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn AXUIElementGetTypeID() -> usize;
        fn AXValueGetTypeID() -> usize;
        fn AXValueGetType(value: CFTypeRef) -> u32;
        fn AXValueGetValue(value: CFTypeRef, typ: u32, ptr: *mut c_void) -> u8;
        fn CFRelease(cf: CFTypeRef);
    }

    const K_AX_VALUE_CFRANGE_TYPE: u32 = 4;

    pub fn is_process_trusted() -> bool {
        unsafe { AXIsProcessTrusted() != 0 }
    }

    pub fn request_trust_prompt() {
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let val = CFBoolean::true_value();
        let opts = CFDictionary::from_CFType_pairs(&[(key, val)]);
        unsafe {
            AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef());
        }
    }

    unsafe fn cfstring_to_string(cf: CFTypeRef) -> Option<String> {
        if cf.is_null() || CFGetTypeID(cf) != CFStringGetTypeID() {
            return None;
        }
        let s = CFString::wrap_under_get_rule(cf as *const _);
        Some(s.to_string())
    }

    unsafe fn as_ax_element(cf: CFTypeRef) -> Option<AXUIElementRef> {
        if cf.is_null() || CFGetTypeID(cf) != AXUIElementGetTypeID() {
            return None;
        }
        Some(cf as AXUIElementRef)
    }

    fn copy_attr(element: AXUIElementRef, name: &str) -> Option<CFType> {
        unsafe {
            let attr = CFString::new(name);
            let mut out: CFTypeRef = std::ptr::null();
            let err = AXUIElementCopyAttributeValue(
                element,
                attr.as_concrete_TypeRef(),
                &mut out,
            );
            if err != AX_K_SUCCESS || out.is_null() {
                return None;
            }
            Some(CFType::wrap_under_create_rule(out))
        }
    }

    fn selected_text_from_element(el: AXUIElementRef) -> Option<String> {
        let from_selected = copy_attr(el, "AXSelectedText").and_then(|cf| {
            let s = unsafe { cfstring_to_string(cf.as_concrete_TypeRef())? };
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        });
        let from_range = selected_text_from_range(el);

        match (from_range, from_selected) {
            (Some(r), Some(s)) => Some(super::pick_longer_multiline(&r, &s)),
            (Some(r), None) => Some(r),
            (None, Some(s)) => Some(s),
            (None, None) => None,
        }
    }

    fn selected_text_from_range(el: AXUIElementRef) -> Option<String> {
        let full_cf = copy_attr(el, "AXValue")?;
        let full_s = unsafe { cfstring_to_string(full_cf.as_concrete_TypeRef())? };
        let range_cf = copy_attr(el, "AXSelectedTextRange")?;
        unsafe {
            if CFGetTypeID(range_cf.as_concrete_TypeRef()) != AXValueGetTypeID() {
                return None;
            }
            if AXValueGetType(range_cf.as_concrete_TypeRef()) != K_AX_VALUE_CFRANGE_TYPE {
                return None;
            }
            #[repr(C)]
            struct CFRange {
                location: isize,
                length: isize,
            }
            let mut range = CFRange {
                location: 0,
                length: 0,
            };
            if AXValueGetValue(
                range_cf.as_concrete_TypeRef(),
                K_AX_VALUE_CFRANGE_TYPE,
                &mut range as *mut _ as *mut c_void,
            ) == 0
            {
                return None;
            }
            if range.location < 0 || range.length <= 0 {
                return None;
            }
            let start = range.location as usize;
            let len = range.length as usize;
            substring_utf16(&full_s, start, len)
        }
    }

    /// 对当前焦点控件执行 AXCopy（Safari/Chrome 等浏览器多行选区更可靠）。
    pub fn perform_ax_copy_on_focused() -> bool {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return false;
            }
            let ok = ax_copy_on_focused_chain(system);
            CFRelease(system as CFTypeRef);
            ok
        }
    }

    fn ax_copy_on_focused_chain(system: AXUIElementRef) -> bool {
        let app_cf = match copy_attr(system, "AXFocusedApplication") {
            Some(cf) => cf,
            None => return false,
        };
        let app_el = unsafe { match as_ax_element(app_cf.as_concrete_TypeRef()) {
            Some(el) => el,
            None => return false,
        }};
        let elem_cf = match copy_attr(app_el, "AXFocusedUIElement") {
            Some(cf) => cf,
            None => return false,
        };
        let mut element = unsafe { match as_ax_element(elem_cf.as_concrete_TypeRef()) {
            Some(el) => el,
            None => return false,
        }};
        let action = CFString::new("AXCopy");
        for _ in 0..12 {
            if ax_perform_copy(element, &action) {
                return true;
            }
            let par_cf = match copy_attr(element, "AXParent") {
                Some(cf) => cf,
                None => break,
            };
            element = unsafe { match as_ax_element(par_cf.as_concrete_TypeRef()) {
                Some(el) => el,
                None => break,
            }};
        }
        false
    }

    fn ax_perform_copy(element: AXUIElementRef, action: &CFString) -> bool {
        unsafe {
            AXUIElementPerformAction(element, action.as_concrete_TypeRef()) == AX_K_SUCCESS
        }
    }

    pub fn focused_app_pid() -> Option<i32> {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return None;
            }
            let result = (|| {
                let app_cf = copy_attr(system, "AXFocusedApplication")?;
                let app_el = as_ax_element(app_cf.as_concrete_TypeRef())?;
                let mut pid: i32 = 0;
                let err = AXUIElementGetPid(app_el, &mut pid);
                if err == AX_K_SUCCESS && pid > 0 {
                    Some(pid)
                } else {
                    None
                }
            })();
            CFRelease(system as CFTypeRef);
            result
        }
    }

    /// 焦点链上是否存在非空选区（不读全文，用于无选区时快速跳过复制）。
    pub fn focused_has_nonempty_selection_range() -> bool {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return false;
            }
            let found = walk_focused_has_selection(system);
            CFRelease(system as CFTypeRef);
            found
        }
    }

    fn element_has_selection(el: AXUIElementRef) -> bool {
        if let Some(cf) = copy_attr(el, "AXSelectedText") {
            if let Some(s) = unsafe { cfstring_to_string(cf.as_concrete_TypeRef()) } {
                if !s.is_empty() {
                    return true;
                }
            }
        }
        let Some(range_cf) = copy_attr(el, "AXSelectedTextRange") else {
            return false;
        };
        unsafe {
            if CFGetTypeID(range_cf.as_concrete_TypeRef()) != AXValueGetTypeID() {
                return false;
            }
            if AXValueGetType(range_cf.as_concrete_TypeRef()) != K_AX_VALUE_CFRANGE_TYPE {
                return false;
            }
            #[repr(C)]
            struct CFRange {
                location: isize,
                length: isize,
            }
            let mut range = CFRange {
                location: 0,
                length: 0,
            };
            if AXValueGetValue(
                range_cf.as_concrete_TypeRef(),
                K_AX_VALUE_CFRANGE_TYPE,
                &mut range as *mut _ as *mut c_void,
            ) == 0
            {
                return false;
            }
            range.length > 0
        }
    }

    fn walk_focused_has_selection(system: AXUIElementRef) -> bool {
        let app_cf = match copy_attr(system, "AXFocusedApplication") {
            Some(cf) => cf,
            None => return false,
        };
        let app_el = unsafe { match as_ax_element(app_cf.as_concrete_TypeRef()) {
            Some(el) => el,
            None => return false,
        }};
        let elem_cf = match copy_attr(app_el, "AXFocusedUIElement") {
            Some(cf) => cf,
            None => return false,
        };
        let mut element = unsafe { match as_ax_element(elem_cf.as_concrete_TypeRef()) {
            Some(el) => el,
            None => return false,
        }};
        for _ in 0..12 {
            if element_has_selection(element) {
                return true;
            }
            let par_cf = match copy_attr(element, "AXParent") {
                Some(cf) => cf,
                None => break,
            };
            element = unsafe { match as_ax_element(par_cf.as_concrete_TypeRef()) {
                Some(el) => el,
                None => break,
            }};
        }
        false
    }

    pub fn selected_text_via_accessibility() -> Option<String> {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return None;
            }
            let result = walk_focused_selection(system);
            CFRelease(system as CFTypeRef);
            result
        }
    }

    /// AX `CFRange` 使用 UTF-16 码元索引（与 NSString 一致）。
    fn substring_utf16(s: &str, location: usize, length: usize) -> Option<String> {
        if length == 0 {
            return None;
        }
        let utf16: Vec<u16> = s.encode_utf16().collect();
        let end = location.checked_add(length)?;
        if end > utf16.len() {
            return None;
        }
        let slice = String::from_utf16(&utf16[location..end]).ok()?;
        if slice.is_empty() {
            None
        } else {
            Some(slice)
        }
    }

    fn walk_focused_selection(system: AXUIElementRef) -> Option<String> {
        let app_cf = copy_attr(system, "AXFocusedApplication")?;
        let app_el = unsafe { as_ax_element(app_cf.as_concrete_TypeRef())? };
        let elem_cf = copy_attr(app_el, "AXFocusedUIElement")?;
        let mut element = unsafe { as_ax_element(elem_cf.as_concrete_TypeRef())? };
        for _ in 0..12 {
            if let Some(t) = selected_text_from_element(element) {
                return Some(t);
            }
            let par_cf = copy_attr(element, "AXParent")?;
            element = unsafe { as_ax_element(par_cf.as_concrete_TypeRef())? };
        }
        None
    }
}

#[cfg(target_os = "macos")]
pub use macos_ax::{is_process_trusted, request_trust_prompt};

#[cfg(not(target_os = "macos"))]
pub fn request_trust_prompt() {}

#[cfg(not(target_os = "macos"))]
pub fn is_process_trusted() -> bool {
    true
}
