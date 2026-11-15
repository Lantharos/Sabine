//! What pages and activities ask of the machine's power: to stay awake, and
//! to hear when the person goes idle.

mod idle_watches;
mod keep_awake;

pub(crate) use idle_watches::IdleWatches;
pub(crate) use keep_awake::keep_awake;
