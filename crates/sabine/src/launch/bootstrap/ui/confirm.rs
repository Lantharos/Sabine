use softbuffer::{Context, Surface};
use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex, mpsc},
};
use winit::{
    application::ApplicationHandler,
    cursor::{Cursor, CursorIcon},
    dpi::{LogicalSize, PhysicalPosition},
    event::{ButtonSource, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

use super::{
    diagnostics,
    dialog_paint::{Document, paint},
};

const WIDTH: f64 = 720.0;

pub(crate) fn confirm_update(app_name: &str, version: &str) -> Result<bool, String> {
    run(
        format!("Update {app_name}"),
        format!(
            "Version {version} is ready to install.\n\nThe app will close while the update is installed. You can choose Later to keep using this version."
        ),
        false,
    )
}

pub(crate) fn show_notice(title: &str, message: &str) -> Result<(), String> {
    run(title.to_owned(), diagnostics::details(message), true).map(|_| ())
}

fn run(title: String, message: String, notice: bool) -> Result<bool, String> {
    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    let outcome = Arc::new(Mutex::new(Ok(false)));
    let app = Dialog {
        window: None,
        context: None,
        surface: None,
        cursor: PhysicalPosition::new(-1.0, -1.0),
        outcome: outcome.clone(),
        title,
        notice,
        document: Document::new(&message),
        focus: 1,
        pressed: None,
        modifiers: ModifiersState::empty(),
        scroll: 0.0,
        max_scroll: 0.0,
        status: String::new(),
        log_result: None,
        proxy: event_loop.create_proxy(),
    };
    event_loop.run_app(app).map_err(|error| error.to_string())?;
    outcome.lock().map_err(|error| error.to_string())?.clone()
}

pub(super) struct Dialog {
    window: Option<Arc<dyn Window>>,
    context: Option<Context<Arc<dyn Window>>>,
    surface: Option<Surface<Arc<dyn Window>, Arc<dyn Window>>>,
    cursor: PhysicalPosition<f64>,
    outcome: Arc<Mutex<Result<bool, String>>>,
    pub(super) title: String,
    pub(super) notice: bool,
    pub(super) document: Document,
    pub(super) focus: usize,
    pub(super) pressed: Option<usize>,
    modifiers: ModifiersState,
    pub(super) scroll: f32,
    pub(super) max_scroll: f32,
    pub(super) status: String,
    log_result: Option<mpsc::Receiver<Result<(), String>>>,
    proxy: EventLoopProxy,
}

impl ApplicationHandler for Dialog {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        self.resumed(event_loop);
    }

    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = WindowAttributes::default()
            .with_title(&self.title)
            .with_surface_size(LogicalSize::new(
                WIDTH,
                if self.notice { 460.0 } else { 300.0 },
            ))
            .with_min_surface_size(LogicalSize::new(
                560.0,
                if self.notice { 360.0 } else { 300.0 },
            ));
        let result = (|| {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(attributes)
                    .map_err(|e| e.to_string())?,
            );
            let context = Context::new(window.clone()).map_err(|e| e.to_string())?;
            let surface = Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
            self.context = Some(context);
            self.surface = Some(surface);
            self.window = Some(window);
            Ok(())
        })();
        if let Err(error) = result {
            self.fail(event_loop, error);
        } else {
            self.redraw(event_loop);
        }
    }

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        if let Some(result) = self
            .log_result
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.log_result = None;
            self.status = result
                .err()
                .unwrap_or_else(|| "Logs opened in your file manager.".into());
            self.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::Focused(false) => {
                self.pressed = None;
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::Tab) => {
                        self.focus = 1 - self.focus;
                        self.request_redraw();
                    }
                    Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) => {
                        self.focus = 1 - self.focus;
                        self.request_redraw();
                    }
                    Key::Named(NamedKey::Enter) if !event.repeat => {
                        self.activate(self.focus, event_loop)
                    }
                    Key::Character(ref key) if key == " " && !event.repeat => {
                        self.activate(self.focus, event_loop)
                    }
                    Key::Named(NamedKey::ArrowDown) => self.scroll_by(36.0),
                    Key::Named(NamedKey::ArrowUp) => self.scroll_by(-36.0),
                    Key::Named(NamedKey::PageDown) => self.scroll_by(self.page_height()),
                    Key::Named(NamedKey::PageUp) => self.scroll_by(-self.page_height()),
                    Key::Named(NamedKey::Home) => self.scroll_by(-self.max_scroll / self.scale()),
                    Key::Named(NamedKey::End) => self.scroll_by(self.max_scroll / self.scale()),
                    Key::Character(ref key)
                        if self.notice
                            && self.modifiers.control_key()
                            && key.eq_ignore_ascii_case("l") =>
                    {
                        self.activate(0, event_loop)
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 40.0,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / self.scale(),
                    _ => return,
                };
                self.scroll_by(-dy);
            }
            WindowEvent::PointerMoved { position, .. } => {
                let before = self.hovered();
                self.cursor = position;
                if self.hovered() != before {
                    if let Some(window) = &self.window {
                        window.set_cursor(Cursor::Icon(if self.hovered().is_some() {
                            CursorIcon::Pointer
                        } else {
                            CursorIcon::Default
                        }));
                    }
                    self.request_redraw();
                }
            }
            WindowEvent::PointerLeft { .. } => {
                self.cursor = PhysicalPosition::new(-1.0, -1.0);
                self.request_redraw();
            }
            WindowEvent::PointerButton {
                state,
                button: ButtonSource::Mouse(MouseButton::Left),
                position,
                ..
            } => {
                self.cursor = position;
                let hovered = self.hovered();
                if state == ElementState::Pressed {
                    self.pressed = hovered;
                    if let Some(index) = hovered {
                        self.focus = index;
                    }
                } else if let Some(index) =
                    self.pressed.take().filter(|index| Some(*index) == hovered)
                {
                    self.activate(index, event_loop);
                }
                self.request_redraw();
            }
            WindowEvent::RedrawRequested
            | WindowEvent::SurfaceResized(_)
            | WindowEvent::ScaleFactorChanged { .. } => self.redraw(event_loop),
            _ => {}
        }
    }
}

impl Dialog {
    pub(super) fn scale(&self) -> f32 {
        self.window
            .as_ref()
            .map_or(1.0, |window| window.scale_factor() as f32)
    }

    pub(super) fn button_rect(&self, index: usize) -> (i32, i32, i32, i32) {
        let size = self.window.as_ref().unwrap().surface_size();
        let scale = self.scale();
        let x = if index == 0 {
            28.0
        } else {
            size.width as f32 / scale - 148.0
        };
        (
            (x * scale) as i32,
            size.height as i32 - (70.0 * scale) as i32,
            (120.0 * scale) as i32,
            (42.0 * scale) as i32,
        )
    }

    pub(super) fn hovered(&self) -> Option<usize> {
        self.window.as_ref()?;
        (0..2).find(|index| {
            let (x, y, w, h) = self.button_rect(*index);
            self.cursor.x >= f64::from(x)
                && self.cursor.x < f64::from(x + w)
                && self.cursor.y >= f64::from(y)
                && self.cursor.y < f64::from(y + h)
        })
    }

    fn page_height(&self) -> f32 {
        self.window.as_ref().map_or(200.0, |window| {
            window.surface_size().height as f32 / self.scale()
                - if self.notice { 286.0 } else { 220.0 }
        })
    }

    fn scroll_by(&mut self, delta: f32) {
        self.scroll = (self.scroll + delta * self.scale()).clamp(0.0, self.max_scroll);
        self.request_redraw();
    }

    fn activate(&mut self, index: usize, event_loop: &dyn ActiveEventLoop) {
        if self.notice && index == 0 {
            if self.log_result.is_some() {
                return;
            }
            let (sender, receiver) = mpsc::channel();
            let proxy = self.proxy.clone();
            self.status = match diagnostics::open_logs(move |result| {
                let _ = sender.send(result);
                proxy.wake_up();
            }) {
                Ok(()) => {
                    self.log_result = Some(receiver);
                    "Opening the logs folder…".into()
                }
                Err(error) => error,
            };
            self.request_redraw();
        } else {
            if let Ok(mut outcome) = self.outcome.lock() {
                *outcome = Ok(!self.notice && index == 1);
            }
            event_loop.exit();
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn fail(&self, event_loop: &dyn ActiveEventLoop, error: String) {
        if let Ok(mut outcome) = self.outcome.lock() {
            *outcome = Err(error);
        }
        event_loop.exit();
    }

    fn redraw(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(window) = &self.window else {
            return;
        };
        let size = window.surface_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        let Some(mut surface) = self.surface.take() else {
            return;
        };
        let result = (|| {
            surface.resize(width, height).map_err(|e| e.to_string())?;
            let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
            paint(self, &mut buffer, (size.width, size.height));
            buffer.present().map_err(|e| e.to_string())
        })();
        self.surface = Some(surface);
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }
}
