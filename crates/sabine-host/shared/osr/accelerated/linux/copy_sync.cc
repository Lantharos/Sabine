#include "osr/accelerated/linux/copy_sync.h"

#include <linux/dma-buf.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include <cstdio>

namespace sabine_osr {
namespace {

constexpr uint64_t kCopyTimeoutNs = 1000000000;

VkSemaphore CreateSemaphore(VulkanContext* vulkan, bool exportable) {
  VkExportSemaphoreCreateInfo exported{
      VK_STRUCTURE_TYPE_EXPORT_SEMAPHORE_CREATE_INFO};
  exported.handleTypes = VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_SYNC_FD_BIT;
  VkSemaphoreCreateInfo info{VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO};
  info.pNext = exportable ? &exported : nullptr;
  VkSemaphore semaphore = VK_NULL_HANDLE;
  vulkan->vkCreateSemaphore(vulkan->device, &info, nullptr, &semaphore);
  return semaphore;
}

bool WaitForProducer(VulkanContext* vulkan, VkSemaphore drawn, int fd) {
  if (!drawn) {
    return false;
  }
  dma_buf_export_sync_file request{DMA_BUF_SYNC_READ, -1};
  if (ioctl(fd, DMA_BUF_IOCTL_EXPORT_SYNC_FILE, &request) != 0) {
    return false;
  }
  VkImportSemaphoreFdInfoKHR import{
      VK_STRUCTURE_TYPE_IMPORT_SEMAPHORE_FD_INFO_KHR};
  import.semaphore = drawn;
  import.flags = VK_SEMAPHORE_IMPORT_TEMPORARY_BIT;
  import.handleType = VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_SYNC_FD_BIT;
  import.fd = request.fd;
  if (vulkan->vkImportSemaphoreFdKHR(vulkan->device, &import) != VK_SUCCESS) {
    close(request.fd);
    return false;
  }
  return true;
}

bool ImportFence(int dmabuf_fd, uint32_t usage, int fence) {
  dma_buf_import_sync_file request{usage, fence};
  return ioctl(dmabuf_fd, DMA_BUF_IOCTL_IMPORT_SYNC_FILE, &request) == 0;
}

// Hands the copy's completion to the dma-bufs it touched: readers of the copy
// wait for it, and so does Chromium's next draw into the frame it read. An
// already finished copy exports no fence.
bool AttachCopyFence(VulkanContext* vulkan,
                     VkSemaphore copied,
                     int source_fd,
                     int target_fd) {
  VkSemaphoreGetFdInfoKHR info{VK_STRUCTURE_TYPE_SEMAPHORE_GET_FD_INFO_KHR};
  info.semaphore = copied;
  info.handleType = VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_SYNC_FD_BIT;
  int fence = -1;
  if (vulkan->vkGetSemaphoreFdKHR(vulkan->device, &info, &fence) !=
      VK_SUCCESS) {
    return false;
  }
  if (fence < 0) {
    return true;
  }
  const bool attached = ImportFence(target_fd, DMA_BUF_SYNC_WRITE, fence) &&
                        ImportFence(source_fd, DMA_BUF_SYNC_READ, fence);
  close(fence);
  return attached;
}

}  // namespace

bool CreateCopySubmission(VulkanContext* vulkan, CopySubmission* submission) {
  VkCommandBufferAllocateInfo buffers{
      VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
  buffers.commandPool = vulkan->command_pool;
  buffers.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
  buffers.commandBufferCount = 1;
  VkFenceCreateInfo fence{VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};
  if (vulkan->vkAllocateCommandBuffers(vulkan->device, &buffers,
                                       &submission->commands) != VK_SUCCESS ||
      vulkan->vkCreateFence(vulkan->device, &fence, nullptr,
                            &submission->done) != VK_SUCCESS) {
    DestroyCopySubmission(vulkan, submission);
    return false;
  }
  if (vulkan->vkImportSemaphoreFdKHR) {
    submission->drawn = CreateSemaphore(vulkan, false);
  }
  if (vulkan->implicit_fences) {
    submission->copied = CreateSemaphore(vulkan, true);
  }
  return true;
}

void DestroyCopySubmission(VulkanContext* vulkan, CopySubmission* submission) {
  FinishCopy(vulkan, submission);
  vulkan->vkDestroySemaphore(vulkan->device, submission->copied, nullptr);
  vulkan->vkDestroySemaphore(vulkan->device, submission->drawn, nullptr);
  vulkan->vkDestroyFence(vulkan->device, submission->done, nullptr);
  if (submission->commands) {
    vulkan->vkFreeCommandBuffers(vulkan->device, vulkan->command_pool, 1,
                                 &submission->commands);
  }
  *submission = CopySubmission{};
}

bool FinishCopy(VulkanContext* vulkan, CopySubmission* submission) {
  if (!submission->pending) {
    return true;
  }
  VkResult result = vulkan->vkWaitForFences(
      vulkan->device, 1, &submission->done, VK_TRUE, kCopyTimeoutNs);
  if (result == VK_TIMEOUT) {
    result = vulkan->vkDeviceWaitIdle(vulkan->device);
  }
  if (result != VK_SUCCESS) {
    return false;
  }
  vulkan->vkResetFences(vulkan->device, 1, &submission->done);
  submission->pending = false;
  return true;
}

bool SubmitCopy(VulkanContext* vulkan,
                CopySubmission* submission,
                std::optional<int> source_fd,
                int target_fd) {
  const VkPipelineStageFlags wait_stage = VK_PIPELINE_STAGE_TRANSFER_BIT;
  VkSubmitInfo submit{VK_STRUCTURE_TYPE_SUBMIT_INFO};
  if (kFramesMayStillBeDrawing && source_fd &&
      WaitForProducer(vulkan, submission->drawn, *source_fd)) {
    submit.waitSemaphoreCount = 1;
    submit.pWaitSemaphores = &submission->drawn;
    submit.pWaitDstStageMask = &wait_stage;
  }
  const bool attaches =
      vulkan->implicit_fences && submission->copied && source_fd;
  if (attaches) {
    submit.signalSemaphoreCount = 1;
    submit.pSignalSemaphores = &submission->copied;
  }
  submit.commandBufferCount = 1;
  submit.pCommandBuffers = &submission->commands;
  if (vulkan->vkQueueSubmit(vulkan->queue, 1, &submit, submission->done) !=
      VK_SUCCESS) {
    return false;
  }
  submission->pending = true;
  if (attaches &&
      AttachCopyFence(vulkan, submission->copied, *source_fd, target_fd)) {
    return true;
  }
  if (attaches) {
    vulkan->implicit_fences = false;
    std::fprintf(
        stderr,
        "Sabine CEF: dma-bufs take no fences here; each frame is copied "
        "before Chromium draws again\n");
    std::fflush(stderr);
  }
  return FinishCopy(vulkan, submission);
}

}  // namespace sabine_osr
