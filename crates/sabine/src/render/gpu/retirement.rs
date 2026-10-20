use std::{
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use super::{RendererError, health::DeviceHealth};

/// Waits for submitted GPU work off the render thread, so release callbacks
/// run as soon as the GPU is done with a frame.
///
/// The thread holds the device too, and may be the last to let go of it.
/// Dropping the poller waits for the thread, so the device is destroyed
/// before the renderer is gone: graphics drivers tear down their own state
/// when the process exits and crash if a device is destroyed meanwhile.
pub(super) struct SubmissionPoller {
    wake: Option<mpsc::SyncSender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl SubmissionPoller {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        health: Arc<DeviceHealth>,
    ) -> Result<Self, RendererError> {
        let (wake, receiver) = mpsc::sync_channel(1);
        let device = device.clone();
        let queue = queue.clone();
        let worker = thread::Builder::new()
            .name("sabine-gpu-retire".into())
            .spawn(move || {
                while receiver.recv().is_ok() && health.failure().is_none() {
                    if let Err(error) = device.poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(Duration::from_secs(5)),
                    }) {
                        health.fail(format!("GPU completion failed: {error}"));
                    }
                }
                drop(queue);
            })
            .map_err(|error| RendererError::Device(error.to_string()))?;
        Ok(Self {
            wake: Some(wake),
            worker: Some(worker),
        })
    }

    pub(super) fn notify(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.try_send(());
        }
    }
}

impl Drop for SubmissionPoller {
    fn drop(&mut self) {
        self.wake = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
