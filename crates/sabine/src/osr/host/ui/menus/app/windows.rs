//! The app menu bar as the window's native Win32 menu bar, under the
//! system's titlebar. Windows draws it in the window's frame, so the page
//! keeps the client area, and Alt and F10 reach it as in other apps.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateMenu, CreatePopupMenu, DestroyMenu, DrawMenuBar, HMENU, MENU_ITEM_FLAGS,
    MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, SetMenu, WM_COMMAND,
};
use windows::core::HSTRING;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

use crate::MenuItem;

/// The first command id of the menu's items, clear of the system menu's.
const FIRST_COMMAND: usize = 0x100;
const SUBCLASS_ID: usize = 0x5ab1;

struct Clicks {
    tags: Rc<RefCell<Vec<usize>>>,
    proxy: EventLoopProxy,
}

pub(super) struct MenuBar {
    window: HWND,
    menu: HMENU,
    clicks: *mut Clicks,
}

impl MenuBar {
    /// Puts `items` on the window as its menu bar. Returns the bar, unless
    /// the window has no handle to put it on, and the ids of its items by
    /// the tag each reports.
    pub(super) fn attach(
        window: &Arc<dyn Window>,
        items: &[MenuItem],
        app: &str,
        tags: Rc<RefCell<Vec<usize>>>,
        proxy: EventLoopProxy,
    ) -> (Option<Self>, Vec<String>) {
        let mut ids = Vec::new();
        let Ok(RawWindowHandle::Win32(handle)) =
            window.window_handle().map(|handle| handle.as_raw())
        else {
            return (None, ids);
        };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        let content = window.surface_size();
        // SAFETY: the menus are built and attached on the window's own thread,
        // and the subclass's data lives until `Drop` removes the subclass.
        unsafe {
            let Ok(menu) = CreateMenu() else {
                return (None, ids);
            };
            fill(menu, items, app, &mut ids);
            let clicks = Box::into_raw(Box::new(Clicks { tags, proxy }));
            let _ = SetWindowSubclass(hwnd, Some(menu_commands), SUBCLASS_ID, clicks as usize);
            let _ = SetMenu(hwnd, Some(menu));
            let _ = DrawMenuBar(hwnd);
            if !window.is_maximized() && window.fullscreen().is_none() {
                let _ = window.request_surface_size(content.into());
            }
            (
                Some(Self {
                    window: hwnd,
                    menu,
                    clicks,
                }),
                ids,
            )
        }
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        // SAFETY: the bar's window and menu are still the ones it attached,
        // and nothing reads the subclass's data once it is removed.
        unsafe {
            let _ = RemoveWindowSubclass(self.window, Some(menu_commands), SUBCLASS_ID);
            let _ = SetMenu(self.window, None);
            let _ = DestroyMenu(self.menu);
            drop(Box::from_raw(self.clicks));
        }
    }
}

/// Appends `items` to `menu`, numbering each command by its place in `ids`.
unsafe fn fill(menu: HMENU, items: &[MenuItem], app: &str, ids: &mut Vec<String>) {
    for item in items
        .iter()
        .filter(|item| item.role.is_none_or(crate::MenuRole::available))
    {
        if item.separator {
            unsafe {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            }
            continue;
        }
        let mut label = item.shown_label(app);
        if let Some(accelerator) = item.shown_accelerator() {
            label = format!("{label}\t{}", accelerator.display());
        }
        let mut flags = MENU_ITEM_FLAGS(0);
        if !item.enabled {
            flags |= MF_GRAYED;
        }
        let label = HSTRING::from(label);
        if !item.items.is_empty() {
            unsafe {
                let Ok(submenu) = CreatePopupMenu() else {
                    continue;
                };
                fill(submenu, &item.items, app, ids);
                let _ = AppendMenuW(menu, flags | MF_POPUP, submenu.0 as usize, &label);
            }
            continue;
        }
        if item.checked == Some(true) {
            flags |= MF_CHECKED;
        }
        unsafe {
            let _ = AppendMenuW(menu, flags | MF_STRING, FIRST_COMMAND + ids.len(), &label);
        }
        ids.push(item.chosen_id().to_string());
    }
}

/// Hands the commands of the menu bar to the window's event loop.
unsafe extern "system" fn menu_commands(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    let from_menu = (wparam.0 >> 16) & 0xffff == 0 && lparam.0 == 0;
    let command = wparam.0 & 0xffff;
    if message == WM_COMMAND && from_menu && command >= FIRST_COMMAND {
        // SAFETY: `data` is the `Clicks` the bar installed with the subclass.
        let clicks = unsafe { &*(data as *const Clicks) };
        clicks.tags.borrow_mut().push(command - FIRST_COMMAND);
        clicks.proxy.wake_up();
        return LRESULT(0);
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}
