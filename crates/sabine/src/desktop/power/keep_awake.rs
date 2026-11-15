use sabine_bridge::{KeepAwake, KeepAwakeGuard};
use sabine_platform::SleepBlocker;
use tokio::sync::oneshot;

/// Keeps the machine awake for an activity while the guard lives. The
/// desktop answers on Sabine's async runtime, and an activity that ended
/// before it answered lets go as soon as it does.
pub(crate) fn keep_awake(keep_awake: KeepAwake, reason: &str) -> KeepAwakeGuard {
    let (release, released) = oneshot::channel::<()>();
    let reason = reason.to_string();
    sabine_platform::async_runtime().spawn(async move {
        let blocker = match keep_awake {
            KeepAwake::System => SleepBlocker::system(&reason).await,
            KeepAwake::Display => SleepBlocker::display(&reason).await,
        };
        match blocker {
            Ok(_blocker) => {
                let _ = released.await;
            }
            Err(error) => sabine_runtime::report_error(
                "power",
                format!("could not keep the machine awake for {reason}: {error}"),
            ),
        }
    });
    KeepAwakeGuard::new(release)
}
