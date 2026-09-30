use std::sync::mpsc;
use std::time::Duration;

use crate::render::BgraImage;

use super::GpuRenderer;

const READBACK_TIMEOUT: Duration = Duration::from_secs(2);

impl GpuRenderer {
    /// Copies the dynamic image `id` back from the GPU, for keeping it while
    /// the renderer goes away or for capturing it.
    pub(crate) fn read_bgra_image(&self, id: &str) -> Option<BgraImage> {
        if self.check_device().is_err() {
            return None;
        }
        let entry = self.texture_cache.get(id).filter(|entry| !entry.external)?;
        let (width, height) = (entry.width, entry.height);
        let row_bytes = width * 4;
        let padded_row_bytes = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sabine-image-readback"),
            size: u64::from(padded_row_bytes) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sabine-image-readback"),
            });
        encoder.copy_texture_to_buffer(
            entry.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(READBACK_TIMEOUT),
            })
            .ok()?;
        receiver.recv_timeout(READBACK_TIMEOUT).ok()?.ok()?;
        let mapped = buffer.slice(..).get_mapped_range().ok()?;
        let mut bytes = Vec::with_capacity(row_bytes as usize * height as usize);
        for row in mapped.chunks_exact(padded_row_bytes as usize) {
            bytes.extend_from_slice(&row[..row_bytes as usize]);
        }
        Some(BgraImage {
            width,
            height,
            bytes,
        })
    }
}
