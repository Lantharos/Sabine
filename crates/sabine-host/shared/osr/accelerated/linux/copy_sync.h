#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_LINUX_COPY_SYNC_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_LINUX_COPY_SYNC_H_

#include <optional>

#include "include/cef_version.h"
#include "osr/accelerated/linux/vulkan_context.h"

namespace sabine_osr {

// Chromium before 158 hands over a frame once its draw is submitted, not once
// it finished (crbug.com/428022619), so a copy waits for the fences on the
// dma-buf and copies again what it took from a frame still being drawn.
constexpr bool kFramesMayStillBeDrawing = CHROME_VERSION_MAJOR < 158;

// What one slot's copies are recorded and ordered with. A slot is copied into
// again only after the window released it, so its last copy has normally long
// finished by then.
struct CopySubmission {
  VkCommandBuffer commands = VK_NULL_HANDLE;
  VkFence done = VK_NULL_HANDLE;
  // Waits for the fences Chromium left on the frame it is still drawing.
  VkSemaphore drawn = VK_NULL_HANDLE;
  // Signalled with the copy and attached to the dma-bufs it touched.
  VkSemaphore copied = VK_NULL_HANDLE;
  bool pending = false;
};

bool CreateCopySubmission(VulkanContext* vulkan, CopySubmission* submission);
void DestroyCopySubmission(VulkanContext* vulkan, CopySubmission* submission);

// Waits for the slot's last copy. A copy that outlasts the timeout makes the
// device finish everything first, so nothing is reused while it still runs;
// false means the device was lost meanwhile.
bool FinishCopy(VulkanContext* vulkan, CopySubmission* submission);

// Submits the recorded copy from Chromium's dma-buf `source_fd` into the
// dma-buf `target_fd`. Chromium takes its buffer back when the paint callback
// returns, so the copy may only still run then if Chromium's next draw into
// it waits for the copy's fence on the dma-buf, which is when the device's
// driver honours such fences. Otherwise, and for a copy from a buffer Chromium
// already took back (no `source_fd`), this waits until the copy finished.
bool SubmitCopy(VulkanContext* vulkan,
                CopySubmission* submission,
                std::optional<int> source_fd,
                int target_fd);

}  // namespace sabine_osr

#endif
