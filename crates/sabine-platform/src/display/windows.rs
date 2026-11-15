// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Windows tells apps about display changes by broadcasting WM_DISPLAYCHANGE,
// WM_SETTINGCHANGE and WM_DPICHANGED to top-level windows, which message-only
// windows never receive. The watcher keeps a hidden top-level window with a
// message loop on a thread of its own, and keeps the wake function in that
// window's user data until the window is destroyed.

use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SetWindowLongPtrW,
    WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_SETTINGCHANGE,
    WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::w;
use winit::monitor::MonitorHandle;

use super::DisplayRect;

type Wake = Box<dyn Fn() + Send + Sync>;

/// Wakes the window whenever Windows connects, disconnects or rearranges a
/// display, or changes its resolution, scale or work area.
pub struct DisplayWatcher {
    window: isize,
    thread: Option<JoinHandle<()>>,
}

impl DisplayWatcher {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (created, window) = mpsc::channel();
        let thread = thread::spawn(move || listen(Box::new(wake), &created));
        Self {
            window: window.recv().unwrap_or_default(),
            thread: Some(thread),
        }
    }
}

impl Drop for DisplayWatcher {
    fn drop(&mut self) {
        if self.window != 0 {
            let window = HWND(self.window as *mut _);
            let _ = unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn listen(wake: Wake, created: &mpsc::Sender<isize>) {
    let class = w!("SabineDisplayWatcher");
    let Ok(module) = (unsafe { GetModuleHandleW(None) }) else {
        return;
    };
    let instance = module.into();
    unsafe {
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        })
    };
    let Ok(window) = (unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            None,
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }) else {
        return;
    };
    let wake = Box::into_raw(Box::new(wake));
    unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, wake as isize) };
    let _ = created.send(window.0 as isize);
    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
        unsafe { DispatchMessageW(&message) };
    }
    drop(unsafe { Box::from_raw(wake) });
}

extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_DPICHANGED => {
            let wake = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const Wake;
            if !wake.is_null() {
                unsafe { (*wake)() };
            }
        }
        WM_CLOSE => {
            let _ = unsafe { DestroyWindow(window) };
            return LRESULT(0);
        }
        WM_DESTROY => unsafe { PostQuitMessage(0) },
        _ => {}
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

pub(super) fn rects(monitor: &MonitorHandle) -> Option<(DisplayRect, Option<DisplayRect>)> {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let monitor = HMONITOR(monitor.native_id() as isize as *mut _);
    unsafe { GetMonitorInfoW(monitor, &mut info) }
        .as_bool()
        .then(|| (rect(info.rcMonitor), Some(rect(info.rcWork))))
}

fn rect(rect: RECT) -> DisplayRect {
    DisplayRect {
        x: rect.left,
        y: rect.top,
        width: rect.right.abs_diff(rect.left),
        height: rect.bottom.abs_diff(rect.top),
    }
}
