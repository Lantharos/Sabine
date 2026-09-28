// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Windows only lets a low-level keyboard hook block system combos such as
// Alt+Tab or the Start menu. The hook swallows Ctrl, Alt and the Windows keys
// and writes their state into this thread's keyboard state, so every other key
// still arrives as a normal window message with the right modifiers and text.
// Shift is never swallowed or the OS would type the wrong characters.

use std::cell::RefCell;

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{
            GetKeyboardState, SetKeyboardState, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU,
            VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            CallNextHookEx, GetForegroundWindow, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED,
            SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
        },
    },
};

use crate::ShortcutModifiers;

const KEY_DOWN: u8 = 0x80;
const EXTENDED_SCAN_CODE: u32 = 0xE000;

/// A modifier key the hook took from the OS, for the window to forward itself.
#[derive(Clone, Copy, Debug)]
pub struct SystemKey {
    /// The DOM `key` value: `Control`, `Alt` or `Meta`.
    pub key: &'static str,
    /// The scan code, with `0xE000` set for extended keys.
    pub scan_code: u32,
    pub pressed: bool,
    pub repeat: bool,
    pub modifiers: ShortcutModifiers,
}

struct Hook {
    handle: HHOOK,
    window: HWND,
    last_key_down: u32,
    on_key: Box<dyn Fn(SystemKey)>,
}

thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

pub(super) struct WindowsInhibitor;

impl WindowsInhibitor {
    pub(super) fn new(window: HWND, on_key: impl Fn(SystemKey) + 'static) -> Result<Self, String> {
        if HOOK.with_borrow(Option::is_some) {
            return Err("Shortcuts are already inhibited".to_string());
        }
        let module = unsafe { GetModuleHandleW(None) }.map_err(|error| error.to_string())?;
        let handle = unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), Some(module.into()), 0)
        }
        .map_err(|error| format!("Could not install the keyboard hook: {error}"))?;
        HOOK.set(Some(Hook {
            handle,
            window,
            last_key_down: 0,
            on_key: Box::new(on_key),
        }));
        Ok(Self)
    }
}

impl Drop for WindowsInhibitor {
    fn drop(&mut self) {
        if let Some(hook) = HOOK.take() {
            let _ = unsafe { UnhookWindowsHookEx(hook.handle) };
        }
        let mut state = [0; 256];
        if unsafe { GetKeyboardState(&mut state) }.is_ok() {
            for key in [
                VK_CONTROL,
                VK_LCONTROL,
                VK_RCONTROL,
                VK_MENU,
                VK_LMENU,
                VK_RMENU,
                VK_LWIN,
                VK_RWIN,
            ] {
                state[usize::from(key.0)] = 0;
            }
            let _ = unsafe { SetKeyboardState(&state) };
        }
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let now = unsafe { windows::Win32::System::SystemInformation::GetTickCount() };
        eprintln!("SABINE_HOOK vk={:#x} msg={:#x} flags={:#x} latency={}ms", info.vkCode, wparam.0, info.flags.0, now.wrapping_sub(info.time));
        let captured = HOOK.with_borrow_mut(|hook| {
            hook.as_mut()
                .is_some_and(|hook| hook.capture(wparam.0 as u32, info))
        });
        if captured {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

impl Hook {
    fn capture(&mut self, message: u32, info: &KBDLLHOOKSTRUCT) -> bool {
        let Some(key) = captured_key(VIRTUAL_KEY(info.vkCode as u16)) else {
            return false;
        };
        if unsafe { GetForegroundWindow() } != self.window {
            return false;
        }
        let pressed = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let repeat = pressed && self.last_key_down == info.vkCode;
        self.last_key_down = if pressed { info.vkCode } else { 0 };
        let Some(modifiers) = update_keyboard_state(info.vkCode as usize, pressed) else {
            return false;
        };
        let extended = if info.flags.contains(LLKHF_EXTENDED) {
            EXTENDED_SCAN_CODE
        } else {
            0
        };
        (self.on_key)(SystemKey {
            key,
            scan_code: (info.scanCode & 0xFF) | extended,
            pressed,
            repeat,
            modifiers,
        });
        true
    }
}

fn captured_key(key: VIRTUAL_KEY) -> Option<&'static str> {
    match key {
        VK_LCONTROL | VK_RCONTROL => Some("Control"),
        VK_LMENU | VK_RMENU => Some("Alt"),
        VK_LWIN | VK_RWIN => Some("Meta"),
        _ => None,
    }
}

fn update_keyboard_state(key: usize, pressed: bool) -> Option<ShortcutModifiers> {
    let mut state = [0; 256];
    unsafe { GetKeyboardState(&mut state) }.ok()?;
    state[key] = if pressed { KEY_DOWN } else { 0 };
    let down = |state: &[u8; 256], key: VIRTUAL_KEY| state[usize::from(key.0)] & KEY_DOWN != 0;
    let control = down(&state, VK_LCONTROL) || down(&state, VK_RCONTROL);
    let alt = down(&state, VK_LMENU) || down(&state, VK_RMENU);
    state[usize::from(VK_CONTROL.0)] = if control { KEY_DOWN } else { 0 };
    state[usize::from(VK_MENU.0)] = if alt { KEY_DOWN } else { 0 };
    unsafe { SetKeyboardState(&state) }.ok()?;
    Some(ShortcutModifiers {
        ctrl: control,
        alt,
        shift: down(&state, VK_SHIFT),
        meta: down(&state, VK_LWIN) || down(&state, VK_RWIN),
    })
}
