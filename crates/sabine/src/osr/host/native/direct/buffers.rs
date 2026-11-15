use std::{os::fd::AsFd, sync::Arc};

use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1;

use super::DirectFrame;
use super::worker::State;
use crate::osr::accel::{Dmabuf, texture_format};
use crate::osr::control::SlotRelease;

const DRM_FORMAT_XRGB8888: u32 = u32::from_le_bytes(*b"XR24");
const DRM_FORMAT_XBGR8888: u32 = u32::from_le_bytes(*b"XB24");
const DRM_FORMAT_ARGB8888: u32 = u32::from_le_bytes(*b"AR24");
const DRM_FORMAT_ABGR8888: u32 = u32::from_le_bytes(*b"AB24");

/// The compositor's buffer for one of the browser's dma-bufs, holding the
/// slot of the frame it shows until the compositor gives the buffer back.
pub(super) struct Buffer {
    pub(super) buffer: WlBuffer,
    pub(super) release: Option<SlotRelease>,
    dmabuf: Arc<Dmabuf>,
}

impl State {
    /// Asks the compositor to import a frame's dma-buf, unless it already
    /// does. Returns whether the import is under way.
    pub(super) fn import(&mut self, frame: &DirectFrame) -> bool {
        let format = match drm_format(frame.format, self.transparent) {
            Ok(format) => format,
            Err(error) => {
                self.fail(error);
                return false;
            }
        };
        if !self.modifiers.contains(&(format, frame.dmabuf.modifier)) {
            self.fail(format!(
                "the compositor cannot import format {format:#x} with modifier {:#x}",
                frame.dmabuf.modifier
            ));
            return false;
        }
        if self.creating.contains_key(&frame.resource_id) {
            return true;
        }
        let dmabuf = &frame.dmabuf;
        let params = self.dmabuf.create_params(&self.handle, frame.resource_id);
        params.add(
            dmabuf.fd.as_fd(),
            0,
            dmabuf.offset,
            dmabuf.stride,
            (dmabuf.modifier >> 32) as u32,
            dmabuf.modifier as u32,
        );
        params.create(
            dmabuf.width as i32,
            dmabuf.height as i32,
            format,
            zwp_linux_buffer_params_v1::Flags::empty(),
        );
        self.creating
            .insert(frame.resource_id, Arc::clone(&frame.dmabuf));
        true
    }

    /// Lets go of the buffers of dma-bufs the browser retired, once the
    /// compositor gave them back.
    pub(super) fn forget_retired_buffers(&mut self) {
        self.buffers.retain(|_, buffer| {
            let retired = Arc::strong_count(&buffer.dmabuf) == 1 && buffer.release.is_none();
            if retired {
                buffer.buffer.destroy();
            }
            !retired
        });
    }

    pub(super) fn buffer_created(&mut self, resource_id: u64, buffer: WlBuffer) {
        let Some(dmabuf) = self.creating.remove(&resource_id) else {
            buffer.destroy();
            return;
        };
        self.buffers.insert(
            resource_id,
            Buffer {
                buffer,
                release: None,
                dmabuf,
            },
        );
        if let Some(frame) = self
            .waiting
            .take_if(|frame| frame.resource_id == resource_id)
        {
            self.show(frame);
        }
    }

    pub(super) fn buffer_failed(&mut self, resource_id: u64) {
        self.creating.remove(&resource_id);
        self.fail("the compositor could not import the page's dma-buf".to_string());
        self.waiting = None;
    }

    pub(super) fn buffer_released(&mut self, released: &WlBuffer) {
        if let Some(buffer) = self
            .buffers
            .values_mut()
            .find(|buffer| &buffer.buffer == released)
        {
            buffer.release = None;
        }
    }
}

fn drm_format(color_type: u32, transparent: bool) -> Result<u32, String> {
    let bgra = texture_format(color_type)? == wgpu::TextureFormat::Bgra8UnormSrgb;
    Ok(match (bgra, transparent) {
        (true, false) => DRM_FORMAT_XRGB8888,
        (false, false) => DRM_FORMAT_XBGR8888,
        (true, true) => DRM_FORMAT_ARGB8888,
        (false, true) => DRM_FORMAT_ABGR8888,
    })
}
