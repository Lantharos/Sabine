//! Launch stages (`host.ready`, `osr_host.spawned.pid.<pid>`, etc.), exposed
//! by `SabineProcess::metrics()` and printed while `SABINE_TRACE` is set.

use std::{
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

static TRACE: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("SABINE_TRACE").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "trace"
        )
    })
});

/// Whether `SABINE_TRACE` asks for launch and host tracing on stderr.
pub(crate) fn trace_enabled() -> bool {
    *TRACE
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SabineLaunchMetric {
    pub stage: String,
    pub elapsed: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SabineLaunchMetricsSnapshot {
    pub label: String,
    pub elapsed: Duration,
    pub stages: Vec<SabineLaunchMetric>,
}

#[derive(Clone, Debug)]
pub(crate) struct LaunchMetrics {
    started: Instant,
    label: String,
    stages: Arc<Mutex<Vec<SabineLaunchMetric>>>,
}

impl LaunchMetrics {
    pub(crate) fn new(label: impl Into<String>) -> Self {
        Self {
            started: Instant::now(),
            label: label.into(),
            stages: Arc::default(),
        }
    }

    pub(crate) fn mark(&self, stage: impl Into<String>) {
        let stage = stage.into();
        let elapsed = self.started.elapsed();
        if trace_enabled() {
            eprintln!(
                "sabine trace [{}] +{}ms {stage}",
                self.label,
                elapsed.as_millis()
            );
        }
        if let Ok(mut stages) = self.stages.lock() {
            stages.push(SabineLaunchMetric { stage, elapsed });
        }
    }

    pub(crate) fn snapshot(&self) -> SabineLaunchMetricsSnapshot {
        SabineLaunchMetricsSnapshot {
            label: self.label.clone(),
            elapsed: self.started.elapsed(),
            stages: self
                .stages
                .lock()
                .map(|stages| stages.clone())
                .unwrap_or_default(),
        }
    }
}
