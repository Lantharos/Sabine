//! What an app leaves behind to find out what went wrong: Chromium's crash
//! reports and the app's log.

mod crash;

pub use crash::{CrashReport, CrashReports};
