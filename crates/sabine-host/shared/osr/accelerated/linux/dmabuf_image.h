#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_LINUX_DMABUF_IMAGE_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_LINUX_DMABUF_IMAGE_H_

#include <cstdint>

#include "include/internal/cef_types_wrappers.h"
#include "osr/accelerated/linux/vulkan_context.h"

namespace sabine_osr {

struct DmabufImage {
  VkImage image = VK_NULL_HANDLE;
  VkDeviceMemory memory = VK_NULL_HANDLE;
  int fd = -1;
  uint32_t width = 0;
  uint32_t height = 0;
  uint64_t modifier = 0;
  uint32_t stride = 0;
  uint32_t offset = 0;
};

bool ImportDmabuf(VulkanContext* vulkan,
                  const CefAcceleratedPaintInfo& info,
                  VkFormat format,
                  DmabufImage* out);
bool CreateExportableDmabuf(VulkanContext* vulkan,
                            SharedFormat format,
                            uint32_t width,
                            uint32_t height,
                            DmabufImage* out);
void DestroyDmabuf(VulkanContext* vulkan, DmabufImage* image);

}  // namespace sabine_osr

#endif
