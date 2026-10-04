use std::sync::Arc;

use crate::window::style::Color;

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayList {
    pub background: Color,
    pub commands: Vec<DisplayCommand>,
}

impl Default for DisplayList {
    fn default() -> Self {
        Self::new(Color::rgba(0.0, 0.0, 0.0, 0.0))
    }
}

impl DisplayList {
    pub fn new(background: Color) -> Self {
        Self {
            background,
            commands: Vec::new(),
        }
    }

    /// Empties the list for the next frame while keeping its storage.
    pub fn reset(&mut self, background: Color) {
        self.background = background;
        self.commands.clear();
    }

    pub fn push(&mut self, command: impl Into<DisplayCommand>) {
        self.commands.push(command.into());
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DisplayCommand {
    Rect(RectCommand),
    RoundedRect(RoundedRectCommand),
    /// Clears what earlier rectangles drew, so surfaces beneath the window show through.
    Cutout(RoundedRectCommand),
    Text(TextCommand),
    Image(ImageCommand),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RectCommand {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoundedRectCommand {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextCommand {
    pub text: Arc<str>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub size: f32,
    pub line_height: f32,
    pub color: Color,
    pub align: TextAlign,
}

/// A browser surface the window shows: the page, or one of the overlays
/// drawn above it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageId {
    Main,
    Overlay(Arc<str>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageCommand {
    pub id: ImageId,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<RectCommand> for DisplayCommand {
    fn from(command: RectCommand) -> Self {
        Self::Rect(command)
    }
}

impl From<RoundedRectCommand> for DisplayCommand {
    fn from(command: RoundedRectCommand) -> Self {
        Self::RoundedRect(command)
    }
}

impl From<TextCommand> for DisplayCommand {
    fn from(command: TextCommand) -> Self {
        Self::Text(command)
    }
}

impl From<ImageCommand> for DisplayCommand {
    fn from(command: ImageCommand) -> Self {
        Self::Image(command)
    }
}
