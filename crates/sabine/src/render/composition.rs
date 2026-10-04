// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// A window gets one DirectComposition target, and only for as long as its
// owner keeps it. The renderer's swapchain and native media both live in this
// tree, so it is shared by window handle and outlives a renderer that is
// rebuilt after GPU recovery while media still shows. Setting a swapchain as a
// visual's content shows nothing until the device commits.

use std::sync::{Arc, Mutex, Weak};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::HWND,
    Graphics::DirectComposition::{
        DCompositionCreateDevice2, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
    },
};
use windows::core::Interface;
use winit::window::Window;

/// A window's composition tree: media visuals beneath the page's swapchain.
pub(crate) struct Composition {
    window: isize,
    device: IDCompositionDevice,
    _target: IDCompositionTarget,
    media: IDCompositionVisual,
    page: IDCompositionVisual,
}

// SAFETY: DirectComposition is free-threaded; every method may be called from
// any thread.
unsafe impl Send for Composition {}
unsafe impl Sync for Composition {}

static WINDOWS: Mutex<Vec<Weak<Composition>>> = Mutex::new(Vec::new());

impl Composition {
    /// The window's composition tree, created when nothing holds it yet.
    pub(crate) fn for_window(window: &dyn Window) -> Result<Arc<Self>, String> {
        let Ok(RawWindowHandle::Win32(handle)) =
            window.window_handle().map(|handle| handle.as_raw())
        else {
            return Err("the window has no Win32 handle".to_string());
        };
        let hwnd = handle.hwnd.get();
        let mut windows = WINDOWS.lock().unwrap_or_else(|error| error.into_inner());
        windows.retain(|tree| tree.strong_count() > 0);
        if let Some(tree) = windows
            .iter()
            .filter_map(Weak::upgrade)
            .find(|tree| tree.window == hwnd)
        {
            return Ok(tree);
        }
        let tree = Arc::new(unsafe { Self::create(hwnd) }.map_err(|error| error.to_string())?);
        windows.push(Arc::downgrade(&tree));
        Ok(tree)
    }

    unsafe fn create(window: isize) -> windows::core::Result<Self> {
        unsafe {
            let device: IDCompositionDevice = DCompositionCreateDevice2(None)?;
            let target = device.CreateTargetForHwnd(HWND(window as _), false)?;
            let root = device.CreateVisual()?;
            let media = device.CreateVisual()?;
            let page = device.CreateVisual()?;
            root.AddVisual(&media, false, None)?;
            root.AddVisual(&page, true, &media)?;
            target.SetRoot(&root)?;
            device.Commit()?;
            Ok(Self {
                window,
                device,
                _target: target,
                media,
                page,
            })
        }
    }

    pub(crate) fn device(&self) -> &IDCompositionDevice {
        &self.device
    }

    /// The visual that holds media surfaces, beneath the page.
    pub(crate) fn media(&self) -> &IDCompositionVisual {
        &self.media
    }

    /// The visual the renderer presents the page into.
    pub(crate) fn page(&self) -> *mut std::ffi::c_void {
        self.page.as_raw()
    }

    pub(crate) fn commit(&self) {
        if let Err(error) = unsafe { self.device.Commit() } {
            eprintln!("Sabine: could not update the window's composition: {error}");
        }
    }
}
