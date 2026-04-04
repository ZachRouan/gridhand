#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use std::ffi::c_void;

// --- Core types ---

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

impl CGRect {
    pub fn null() -> Self {
        CGRect {
            origin: CGPoint { x: f64::INFINITY, y: f64::INFINITY },
            size: CGSize { width: 0.0, height: 0.0 },
        }
    }
}

// --- CGEvent constants ---

// Event types
pub const kCGEventMouseMoved: u32 = 5;
pub const kCGEventLeftMouseDown: u32 = 1;
pub const kCGEventLeftMouseUp: u32 = 2;
pub const kCGEventRightMouseDown: u32 = 3;
pub const kCGEventRightMouseUp: u32 = 4;
pub const kCGEventLeftMouseDragged: u32 = 6;

// Mouse buttons
pub const kCGMouseButtonLeft: u32 = 0;
pub const kCGMouseButtonRight: u32 = 1;

// Event tap location
pub const kCGHIDEventTap: u32 = 0;

// --- CGWindow constants ---

pub const kCGWindowListOptionOnScreenOnly: u32 = 1 << 0;
pub const kCGWindowListOptionIncludingWindow: u32 = 1 << 3;
pub const kCGWindowImageDefault: u32 = 0;
pub const kCGWindowImageBoundsIgnoreFraming: u32 = 1 << 0;
pub const kCGNullWindowID: u32 = 0;

// --- CFNumber types ---

pub const kCFNumberSInt32Type: i32 = 3;
pub const kCFNumberSInt64Type: i32 = 4;

// --- CFString encoding ---

pub const kCFStringEncodingUTF8: u32 = 0x08000100;

// --- CGImage bitmap info ---

pub const kCGBitmapByteOrderDefault: u32 = 0;
pub const kCGBitmapByteOrder32Little: u32 = 2 << 12;
pub const kCGImageAlphaPremultipliedFirst: u32 = 2;
pub const kCGImageAlphaPremultipliedLast: u32 = 1;
pub const kCGImageAlphaNoneSkipFirst: u32 = 6;
pub const kCGImageAlphaNoneSkipLast: u32 = 5;

// --- NSApplication activation options ---

pub const NSApplicationActivateIgnoringOtherApps: u64 = 1 << 1;

// --- CoreGraphics FFI ---

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    // Events
    pub fn CGEventCreateMouseEvent(
        source: *const c_void,
        mouseType: u32,
        mouseCursorPosition: CGPoint,
        mouseButton: u32,
    ) -> *mut c_void;

    pub fn CGEventCreateKeyboardEvent(
        source: *const c_void,
        virtualKey: u16,
        keyDown: bool,
    ) -> *mut c_void;

    pub fn CGEventPost(tap: u32, event: *mut c_void);

    // Screenshots
    pub fn CGWindowListCreateImage(
        screenBounds: CGRect,
        listOption: u32,
        windowID: u32,
        imageOption: u32,
    ) -> *mut c_void;

    pub fn CGWindowListCopyWindowInfo(
        option: u32,
        relativeToWindow: u32,
    ) -> *mut c_void;

    // CGImage
    pub fn CGImageGetWidth(image: *const c_void) -> usize;
    pub fn CGImageGetHeight(image: *const c_void) -> usize;
    pub fn CGImageGetBytesPerRow(image: *const c_void) -> usize;
    pub fn CGImageGetBitmapInfo(image: *const c_void) -> u32;
    pub fn CGImageGetBitsPerPixel(image: *const c_void) -> usize;
    pub fn CGImageGetDataProvider(image: *const c_void) -> *mut c_void;

    // CGDataProvider
    pub fn CGDataProviderCopyData(provider: *const c_void) -> *mut c_void;
}

// --- CoreFoundation FFI ---

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFRelease(cf: *mut c_void);

    // CFArray
    pub fn CFArrayGetCount(array: *const c_void) -> i64;
    pub fn CFArrayGetValueAtIndex(array: *const c_void, idx: i64) -> *const c_void;

    // CFDictionary
    pub fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;

    // CFNumber
    pub fn CFNumberGetValue(
        number: *const c_void,
        theType: i32,
        valuePtr: *mut c_void,
    ) -> bool;

    // CFString
    pub fn CFStringGetCString(
        string: *const c_void,
        buffer: *mut u8,
        bufferSize: i64,
        encoding: u32,
    ) -> bool;

    // CFData
    pub fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
    pub fn CFDataGetLength(data: *const c_void) -> i64;

    // CFBoolean
    pub static kCFBooleanTrue: *const c_void;
    pub static kCFBooleanFalse: *const c_void;
}

// --- Window dictionary keys ---

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub static kCGWindowNumber: *const c_void;
    pub static kCGWindowOwnerPID: *const c_void;
    pub static kCGWindowName: *const c_void;
    pub static kCGWindowOwnerName: *const c_void;
    pub static kCGWindowBounds: *const c_void;
    pub static kCGWindowLayer: *const c_void;
}

// --- Objective-C runtime ---

#[link(name = "objc", kind = "dylib")]
extern "C" {
    pub fn objc_getClass(name: *const u8) -> *mut c_void;
    pub fn sel_registerName(name: *const u8) -> *mut c_void;
    pub fn objc_msgSend(receiver: *mut c_void, selector: *mut c_void, ...) -> *mut c_void;
}

// --- Helpers ---

/// Read a CFString into a Rust String. Returns None if the pointer is null or conversion fails.
pub fn cfstring_to_string(cfstr: *const c_void) -> Option<String> {
    if cfstr.is_null() {
        return None;
    }
    let mut buf = [0u8; 512];
    let ok = unsafe {
        CFStringGetCString(cfstr, buf.as_mut_ptr(), buf.len() as i64, kCFStringEncodingUTF8)
    };
    if !ok {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..len]).to_string())
}

/// Read a CFNumber (i32) from a pointer. Returns None if null.
pub fn cfnumber_to_i32(cfnum: *const c_void) -> Option<i32> {
    if cfnum.is_null() {
        return None;
    }
    let mut val: i32 = 0;
    let ok = unsafe {
        CFNumberGetValue(cfnum, kCFNumberSInt32Type, &mut val as *mut i32 as *mut c_void)
    };
    if ok { Some(val) } else { None }
}

/// Read a CFNumber (i64) from a pointer. Returns None if null.
pub fn cfnumber_to_i64(cfnum: *const c_void) -> Option<i64> {
    if cfnum.is_null() {
        return None;
    }
    let mut val: i64 = 0;
    let ok = unsafe {
        CFNumberGetValue(cfnum, kCFNumberSInt64Type, &mut val as *mut i64 as *mut c_void)
    };
    if ok { Some(val) } else { None }
}
