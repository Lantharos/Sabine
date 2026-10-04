use std::hash::{DefaultHasher, Hash, Hasher};

use winit::{
    cursor::{Cursor, CursorIcon, CustomCursor, CustomCursorSource},
    event_loop::ActiveEventLoop,
};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::{CursorImage, PageCursorMessage};

/// What the page asks the pointer to look like.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::osr::host) enum PageCursor {
    Icon(CursorIcon),
    Hidden,
    Custom(CustomCursor),
}

impl Default for PageCursor {
    fn default() -> Self {
        Self::Icon(CursorIcon::Default)
    }
}

/// The page cursor, the window's own cursor over its titlebar and edges, and
/// what the window currently shows.
#[derive(Default)]
pub(in crate::osr::host) struct CursorState {
    page: PageCursor,
    native: Option<CursorIcon>,
    applied: Option<PageCursor>,
    custom: Option<(u64, CustomCursor)>,
}

impl CursorState {
    pub(in crate::osr::host) fn forget_window(&mut self) {
        self.native = None;
        self.applied = None;
    }
}

const CEF_CURSOR_NONE: u32 = 37;

impl OsrNativeHost {
    pub(in crate::osr::host) fn set_native_cursor(&mut self, cursor: CursorIcon) {
        self.cursor.native = Some(cursor);
        self.apply_cursor();
    }

    pub(in crate::osr::host) fn clear_native_cursor(&mut self) {
        if self.cursor.native.take().is_some() {
            self.apply_cursor();
        }
    }

    pub(in crate::osr::host) fn set_page_cursor(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        message: PageCursorMessage,
    ) {
        self.cursor.page = match message {
            PageCursorMessage::Named(CEF_CURSOR_NONE) => PageCursor::Hidden,
            PageCursorMessage::Named(kind) => PageCursor::Icon(cef_cursor_icon(kind)),
            PageCursorMessage::Custom(image) => self.custom_cursor(event_loop, image),
        };
        self.apply_cursor();
    }

    fn custom_cursor(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        image: CursorImage,
    ) -> PageCursor {
        let mut hasher = DefaultHasher::new();
        image.hash(&mut hasher);
        let key = hasher.finish();
        if let Some((cached, cursor)) = &self.cursor.custom
            && *cached == key
        {
            return PageCursor::Custom(cursor.clone());
        }
        let source = CustomCursorSource::from_rgba(
            image.rgba,
            image.width,
            image.height,
            image.hotspot_x,
            image.hotspot_y,
        );
        match source.map(|source| event_loop.create_custom_cursor(source)) {
            Ok(Ok(cursor)) => {
                self.cursor.custom = Some((key, cursor.clone()));
                PageCursor::Custom(cursor)
            }
            _ => PageCursor::default(),
        }
    }

    fn apply_cursor(&mut self) {
        let wanted = self
            .cursor
            .native
            .map_or_else(|| self.cursor.page.clone(), PageCursor::Icon);
        if self.cursor.applied.as_ref() == Some(&wanted) {
            return;
        }
        let Some(window) = &self.window else {
            return;
        };
        let hidden = wanted == PageCursor::Hidden;
        if hidden != (self.cursor.applied == Some(PageCursor::Hidden)) {
            window.set_cursor_visible(!hidden);
        }
        match &wanted {
            PageCursor::Icon(icon) => window.set_cursor(Cursor::Icon(*icon)),
            PageCursor::Custom(cursor) => window.set_cursor(Cursor::Custom(cursor.clone())),
            PageCursor::Hidden => {}
        }
        self.cursor.applied = Some(wanted);
    }
}

fn cef_cursor_icon(kind: u32) -> CursorIcon {
    match kind {
        1 => CursorIcon::Crosshair,
        2 => CursorIcon::Pointer,
        3 => CursorIcon::Text,
        4 => CursorIcon::Wait,
        5 => CursorIcon::Help,
        6 | 21 => CursorIcon::EResize,
        7 | 22 => CursorIcon::NResize,
        8 | 23 => CursorIcon::NeResize,
        9 | 24 => CursorIcon::NwResize,
        10 | 25 => CursorIcon::SResize,
        11 | 26 => CursorIcon::SeResize,
        12 | 27 => CursorIcon::SwResize,
        13 | 28 => CursorIcon::WResize,
        14 | 43 => CursorIcon::NsResize,
        15 | 44 => CursorIcon::EwResize,
        16 => CursorIcon::NeswResize,
        17 => CursorIcon::NwseResize,
        18 => CursorIcon::ColResize,
        19 => CursorIcon::RowResize,
        20 => CursorIcon::AllScroll,
        29 | 47 => CursorIcon::Move,
        30 => CursorIcon::VerticalText,
        31 => CursorIcon::Cell,
        32 => CursorIcon::ContextMenu,
        33 | 49 => CursorIcon::Alias,
        34 => CursorIcon::Progress,
        35 | 46 => CursorIcon::NoDrop,
        36 | 48 => CursorIcon::Copy,
        38 => CursorIcon::NotAllowed,
        39 => CursorIcon::ZoomIn,
        40 => CursorIcon::ZoomOut,
        41 => CursorIcon::Grab,
        42 => CursorIcon::Grabbing,
        _ => CursorIcon::Default,
    }
}
