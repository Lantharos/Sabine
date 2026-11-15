use std::{fmt, sync::Arc};

/// How awake an activity keeps the machine while it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeepAwake {
    /// The system stays awake; the screen may still turn off.
    System,
    /// The screen stays on, and so the system awake.
    Display,
}

impl KeepAwake {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Display => "display",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "display" => Some(Self::Display),
            _ => None,
        }
    }
}

/// Whatever keeps the machine awake for an activity, until it is dropped.
pub struct KeepAwakeGuard {
    _held: Box<dyn Send>,
}

impl KeepAwakeGuard {
    pub fn new(held: impl Send + 'static) -> Self {
        Self {
            _held: Box::new(held),
        }
    }
}

impl fmt::Debug for KeepAwakeGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("KeepAwakeGuard")
    }
}

type Wake = dyn Fn(KeepAwake, &str) -> KeepAwakeGuard + Send + Sync;

/// What a registry keeps the machine awake with. A registry without one
/// records how awake its activities would keep the machine and nothing
/// more.
#[derive(Clone, Default)]
pub(super) struct Waker(Option<Arc<Wake>>);

impl Waker {
    pub(super) fn new(
        wake: impl Fn(KeepAwake, &str) -> KeepAwakeGuard + Send + Sync + 'static,
    ) -> Self {
        Self(Some(Arc::new(wake)))
    }

    pub(super) fn wake(&self, keep_awake: KeepAwake, reason: &str) -> Option<KeepAwakeGuard> {
        self.0.as_ref().map(|wake| wake(keep_awake, reason))
    }
}

impl fmt::Debug for Waker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Waker")
    }
}
