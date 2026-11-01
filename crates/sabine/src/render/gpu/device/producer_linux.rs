use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd};

use ash::{khr, vk};
use wgpu::hal::api::Vulkan;

use crate::render::{GpuRenderer, RendererError};

const DMA_BUF_SYNC_READ: u32 = 1;
const DMA_BUF_IOCTL_EXPORT_SYNC_FILE: libc::Ioctl = 0xC008_6202;

#[repr(C)]
struct DmaBufExportSyncFile {
    flags: u32,
    fd: i32,
}

/// Opens the window's device, able to wait for the browser host's copies on
/// the GPU when the driver imports sync files.
pub(in crate::render::gpu) async fn open_device(
    adapter: &wgpu::Adapter,
    descriptor: &wgpu::DeviceDescriptor<'_>,
) -> Result<(wgpu::Device, wgpu::Queue), RendererError> {
    let opened = {
        let Some(hal_adapter) = (unsafe { adapter.as_hal::<Vulkan>() }) else {
            return adapter
                .request_device(descriptor)
                .await
                .map_err(|error| RendererError::Device(error.to_string()));
        };
        let waits = imports_sync_files(&hal_adapter);
        let add_extension: Box<wgpu::hal::vulkan::CreateDeviceCallback> =
            Box::new(move |arguments| {
                if waits {
                    arguments.extensions.push(khr::external_semaphore_fd::NAME);
                }
            });
        unsafe {
            hal_adapter.open_with_callback(
                descriptor.required_features,
                &descriptor.required_limits,
                &descriptor.memory_hints,
                Some(add_extension),
            )
        }
        .map_err(|error| RendererError::Device(error.to_string()))?
    };
    unsafe { adapter.create_device_from_hal(opened, descriptor) }
        .map_err(|error| RendererError::Device(error.to_string()))
}

fn imports_sync_files(adapter: &wgpu::hal::vulkan::Adapter) -> bool {
    if !adapter
        .physical_device_capabilities()
        .supports_extension(khr::external_semaphore_fd::NAME)
    {
        return false;
    }
    let info = vk::PhysicalDeviceExternalSemaphoreInfo::default()
        .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
    let mut properties = vk::ExternalSemaphoreProperties::default();
    unsafe {
        adapter
            .shared_instance()
            .raw_instance()
            .get_physical_device_external_semaphore_properties(
                adapter.raw_physical_device(),
                &info,
                &mut properties,
            );
    }
    properties
        .external_semaphore_features
        .contains(vk::ExternalSemaphoreFeatureFlags::IMPORTABLE)
}

/// The browser host attaches the fence of its copy to the dma-buf it copied
/// into. Before the window first samples a frame, the next submission waits
/// for that fence on the GPU.
pub(in crate::render::gpu) struct ProducerWaits {
    queue: wgpu::Queue,
    device: ash::Device,
    sync_files: Option<khr::external_semaphore_fd::Device>,
    staged: Vec<vk::Semaphore>,
}

impl ProducerWaits {
    pub(in crate::render::gpu) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        let hal_device = unsafe { device.as_hal::<Vulkan>() }?;
        let sync_files = hal_device
            .enabled_device_extensions()
            .contains(&khr::external_semaphore_fd::NAME)
            .then(|| {
                khr::external_semaphore_fd::Device::new(
                    hal_device.shared_instance().raw_instance(),
                    hal_device.raw_device(),
                )
            });
        Some(Self {
            queue: queue.clone(),
            device: hal_device.raw_device().clone(),
            sync_files,
            staged: Vec::new(),
        })
    }

    pub(in crate::render::gpu) fn wait_for_dmabuf(&mut self, dmabuf: BorrowedFd<'_>) {
        let Some(sync_files) = &self.sync_files else {
            return;
        };
        let Some(fence) = export_write_fence(dmabuf) else {
            return;
        };
        let Ok(semaphore) = (unsafe {
            self.device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
        }) else {
            return;
        };
        let import = vk::ImportSemaphoreFdInfoKHR::default()
            .semaphore(semaphore)
            .flags(vk::SemaphoreImportFlags::TEMPORARY)
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
            .fd(fence.as_raw_fd());
        if unsafe { sync_files.import_semaphore_fd(&import) }.is_err() {
            unsafe { self.device.destroy_semaphore(semaphore, None) };
            return;
        }
        let _ = fence.into_raw_fd();
        if let Some(hal_queue) = unsafe { self.queue.as_hal::<Vulkan>() } {
            hal_queue.add_wait_semaphore(semaphore, None, vk::PipelineStageFlags::FRAGMENT_SHADER);
        }
        self.staged.push(semaphore);
    }

    /// Frees the semaphores a submission just made after waiting on them,
    /// once the GPU finished it. Returns whether any waits for that.
    pub(in crate::render::gpu) fn submitted(&mut self) -> bool {
        if self.staged.is_empty() {
            return false;
        }
        let semaphores = std::mem::take(&mut self.staged);
        let device = self.device.clone();
        self.queue.on_submitted_work_done(move || {
            for semaphore in semaphores {
                unsafe { device.destroy_semaphore(semaphore, None) };
            }
        });
        true
    }
}

impl GpuRenderer {
    /// Makes the next submission wait for the copy the browser host made
    /// into `dmabuf`.
    pub(crate) fn wait_for_dmabuf(&mut self, dmabuf: BorrowedFd<'_>) {
        if let Some(waits) = &mut self.producer_waits {
            waits.wait_for_dmabuf(dmabuf);
        }
    }
}

impl Drop for ProducerWaits {
    fn drop(&mut self) {
        let hal_queue = unsafe { self.queue.as_hal::<Vulkan>() };
        for semaphore in self.staged.drain(..) {
            if let Some(hal_queue) = &hal_queue {
                hal_queue.remove_wait_semaphore(semaphore);
            }
            unsafe { self.device.destroy_semaphore(semaphore, None) };
        }
    }
}

fn export_write_fence(dmabuf: BorrowedFd<'_>) -> Option<OwnedFd> {
    let mut request = DmaBufExportSyncFile {
        flags: DMA_BUF_SYNC_READ,
        fd: -1,
    };
    let exported = unsafe {
        libc::ioctl(
            dmabuf.as_raw_fd(),
            DMA_BUF_IOCTL_EXPORT_SYNC_FILE,
            &mut request,
        )
    };
    (exported == 0 && request.fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(request.fd) })
}
