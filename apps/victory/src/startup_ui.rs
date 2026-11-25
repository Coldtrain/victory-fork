//! Native Win32 loading overlay parented to the emulated title's main window.

use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HDC, InvalidateRect,
        PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor, TextOutW, TRANSPARENT, UpdateWindow,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::VK_RETURN, WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
        DestroyWindow, DispatchMessageW, GetMessageW, IDC_ARROW,
        GetClientRect, LoadCursorW, MSG, PostQuitMessage, RegisterClassW, SetTimer,
        TranslateMessage, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_PAINT,
        WM_TIMER, WNDCLASSW, WS_CHILD, WS_VISIBLE,
    }},
};

const WIDTH: i32 = 960;
const HEIGHT: i32 = 540;
const TIMER_ID: usize = 1;

static STOP: OnceLock<Arc<AtomicBool>> = OnceLock::new();
static LAST_DISCOVERED: AtomicU64 = AtomicU64::new(u64::MAX);
static LAST_CHANGE: OnceLock<std::sync::Mutex<Instant>> = OnceLock::new();
static BYPASSED: AtomicBool = AtomicBool::new(false);

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn brush(hdc: HDC, rect: RECT, color: u32) {
    unsafe {
        let b = CreateSolidBrush(color);
        FillRect(hdc, &rect, b);
        DeleteObject(b);
    }
}

fn label(hdc: HDC, x: i32, y: i32, value: &str, color: u32, size: i32, bold: bool) {
    let text = wide(value);
    unsafe {
        let font = windows_sys::Win32::Graphics::Gdi::CreateFontW(
            -size, 0, 0, 0, if bold { 700 } else { 400 }, 0, 0, 0, 1, 0, 0,  clear_type(),
            0, wide("Segoe UI").as_ptr(),
        );
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, color);
        TextOutW(hdc, x, y, text.as_ptr(), (text.len() - 1) as i32);
        SelectObject(hdc, old);
        DeleteObject(font);
    }
}

fn clear_type() -> u32 { 5 }

fn draw(hwnd: HWND) {
    let p = victory_hle::startup_progress();
    let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    let full = RECT { left: 0, top: 0, right: WIDTH, bottom: HEIGHT };
    brush(hdc, full, 0x00100D0B);

    // Subtle header and footer rules.
    brush(hdc, RECT { left: 0, top: 0, right: WIDTH, bottom: 5 }, 0x00D49A42);
    label(hdc, 80, 66, "VICTORY", 0x00FFFFFF, 24, true);
    label(hdc, 80, 112, "SHADER & PIPELINE PRECOMPILATION", 0x00D49A42, 15, true);
    label(hdc, 80, 174, "Preparing the host GPU before the first frame", 0x00D8D4D0, 18, false);

    let pipeline_total = p.discovered;
    let pipeline_done = p.completed.min(pipeline_total);
    let ratio = if pipeline_total == 0 { 0.0 } else {
        pipeline_done as f64 / pipeline_total as f64
    };
    let bar = RECT { left: 80, top: 244, right: 880, bottom: 260 };
    brush(hdc, bar, 0x00302B27);
    if ratio > 0.0 {
        brush(hdc, RECT {
            left: 80, top: 244,
            right: 80 + (800.0 * ratio.clamp(0.0, 1.0)) as i32,
            bottom: 260,
        }, 0x00D49A42);
    }
    label(hdc, 80, 278, &format!("COMPILING PIPELINES  {pipeline_done}/{pipeline_total}"), 0x00FFFFFF, 18, true);
    label(hdc, 80, 318, &format!("Pending: {}     Failed: {}", p.queued, p.failed), 0x00B9B2AC, 14, false);

    let shader_done = p.shaders_cached.saturating_add(p.shaders_translated).min(p.shaders_discovered);
    if p.shaders_discovered == 0 {
        label(hdc, 80, 366, "LOADING SHADERS  Discovering shader containers...", 0x00D8D4D0, 16, true);
    } else {
        label(hdc, 80, 366, &format!("LOADING SHADERS  {shader_done}/{}   ({} cached, {} translated)", p.shaders_discovered, p.shaders_cached, p.shaders_translated), 0x00D8D4D0, 16, true);
    }
    label(hdc, 80, 408, &format!("HOST PIPELINES  {} loaded from library   |   {} compiled", p.library_loaded, p.library_compiled), 0x00B9B2AC, 14, false);

    let status = if BYPASSED.load(Ordering::Acquire) {
        "Waiting bypassed — compilation continues in the background"
    } else if p.failed > 0 {
        "Pipeline failures detected — press ENTER to continue"
    } else if p.discovery_complete {
        "Finishing the initial pipeline batch..."
    } else {
        "Discovering pipelines created during title startup..."
    };
    label(hdc, 80, 466, status, 0x00D49A42, 14, true);
    label(hdc, 80, 500, "Press ENTER to skip the wait", 0x008E8882, 12, false);
    unsafe { EndPaint(hwnd, &ps) };
}

unsafe extern "system" fn window_proc(
    hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            draw(hwnd);
            0
        }
        WM_TIMER => {
            if STOP.get().is_some_and(|s| s.load(Ordering::Acquire)) {
                unsafe { DestroyWindow(hwnd) };
                return 0;
            }
            let p = victory_hle::startup_progress();
            let old = LAST_DISCOVERED.swap(p.discovered, Ordering::AcqRel);
            if old != p.discovered {
                *LAST_CHANGE.get_or_init(|| std::sync::Mutex::new(Instant::now()))
                    .lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
            }
            let precompile_disabled = std::env::var_os("VICTORY_PIPELINE_PRECOMPILE")
                .is_some_and(|v| v == "0");
            let stable = LAST_CHANGE.get_or_init(|| std::sync::Mutex::new(Instant::now()))
                .lock().unwrap_or_else(|e| e.into_inner()).elapsed() >= Duration::from_secs(5);
            if precompile_disabled || (
                p.discovery_complete && p.completed >= p.discovered && p.failed == 0
                && p.queued == 0 && stable
            ) {
                unsafe { DestroyWindow(hwnd) };
                return 0;
            }
            unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
            0
        }
        WM_KEYDOWN if wparam as u32 == VK_RETURN as u32 => {
            BYPASSED.store(true, Ordering::Release);
            victory_hle::bypass_startup_wait();
            unsafe { DestroyWindow(hwnd) };
            0
        }
        WM_CLOSE => {
            BYPASSED.store(true, Ordering::Release);
            victory_hle::bypass_startup_wait();
            unsafe { DestroyWindow(hwnd) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

pub(super) fn run(stop: Arc<AtomicBool>) {
    let _ = STOP.set(stop);
    LAST_DISCOVERED.store(u64::MAX, Ordering::Release);
    BYPASSED.store(false, Ordering::Release);
    let class = wide("VictoryShaderLoadingWindow");
    let title = wide("Victory — Shader and Pipeline Precompilation");
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 { return; }
        // Wait for the first host present to create the actual game window.
        // The pipeline barrier holds that present, leaving this thread free to
        // attach the loading UI as a child of the game window.
        let parent = loop {
            if STOP.get().is_some_and(|s| s.load(Ordering::Acquire)) {
                return;
            }
            if let Some(parent) = victory_hle::startup_parent_window() {
                break parent as HWND;
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(parent, &mut client);
        let parent_width = client.right - client.left;
        let parent_height = client.bottom - client.top;
        let x = (parent_width - WIDTH) / 2;
        let y = (parent_height - HEIGHT) / 2;
        let width = WIDTH.min(parent_width.max(1));
        let height = HEIGHT.min(parent_height.max(1));
        let hwnd = CreateWindowExW(
            0, class.as_ptr(), title.as_ptr(), WS_CHILD | WS_VISIBLE,
            x.max(0), y.max(0), width, height, parent,
            std::ptr::null_mut(), instance, std::ptr::null(),
        );
        if hwnd.is_null() { return; }
        SetTimer(hwnd, TIMER_ID, 200, None);
        UpdateWindow(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
