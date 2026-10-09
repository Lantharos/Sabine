#include "osr/accelerated/linux/dmabuf_image.h"

#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

#include <array>

namespace sabine_osr {
namespace {

bool SameBuffer(int first, int second) {
  struct stat a{};
  struct stat b{};
  return fstat(first, &a) == 0 && fstat(second, &b) == 0 &&
         a.st_dev == b.st_dev && a.st_ino == b.st_ino;
}

bool FindMemoryType(const VulkanContext* vulkan,
                    uint32_t type_bits,
                    VkMemoryPropertyFlags required,
                    uint32_t* out) {
  for (uint32_t index = 0; index < vulkan->memory.memoryTypeCount; ++index) {
    if ((type_bits & (1u << index)) &&
        (vulkan->memory.memoryTypes[index].propertyFlags & required) ==
            required) {
      *out = index;
      return true;
    }
  }
  return false;
}

VkImageCreateInfo ImageInfo(const void* next,
                            VkFormat format,
                            uint32_t width,
                            uint32_t height,
                            VkImageUsageFlags usage) {
  VkImageCreateInfo info{VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO};
  info.pNext = next;
  info.imageType = VK_IMAGE_TYPE_2D;
  info.format = format;
  info.extent = {width, height, 1};
  info.mipLevels = 1;
  info.arrayLayers = 1;
  info.samples = VK_SAMPLE_COUNT_1_BIT;
  info.tiling = VK_IMAGE_TILING_DRM_FORMAT_MODIFIER_EXT;
  info.usage = usage;
  info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
  info.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED;
  return info;
}

bool AllocateDedicated(VulkanContext* vulkan,
                       DmabufImage* image,
                       void* next,
                       uint32_t type_bits,
                       VkMemoryPropertyFlags required) {
  VkMemoryRequirements requirements;
  vulkan->vkGetImageMemoryRequirements(vulkan->device, image->image,
                                       &requirements);
  uint32_t type = 0;
  if (!FindMemoryType(vulkan, requirements.memoryTypeBits & type_bits, required,
                      &type)) {
    return false;
  }
  VkMemoryDedicatedAllocateInfo dedicated{
      VK_STRUCTURE_TYPE_MEMORY_DEDICATED_ALLOCATE_INFO};
  dedicated.pNext = next;
  dedicated.image = image->image;
  VkMemoryAllocateInfo allocate{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
  allocate.pNext = &dedicated;
  allocate.allocationSize = requirements.size;
  allocate.memoryTypeIndex = type;
  return vulkan->vkAllocateMemory(vulkan->device, &allocate, nullptr,
                                  &image->memory) == VK_SUCCESS &&
         vulkan->vkBindImageMemory(vulkan->device, image->image, image->memory,
                                   0) == VK_SUCCESS;
}

}  // namespace

bool ImportDmabuf(VulkanContext* vulkan,
                  const CefAcceleratedPaintInfo& info,
                  VkFormat format,
                  DmabufImage* out) {
  const int planes = info.plane_count;
  if (planes < 1 || planes > kAcceleratedPaintMaxPlanes) {
    return false;
  }
  std::array<VkSubresourceLayout, kAcceleratedPaintMaxPlanes> layouts{};
  for (int plane = 0; plane < planes; ++plane) {
    if (!SameBuffer(info.planes[0].fd, info.planes[plane].fd)) {
      return false;
    }
    layouts[plane].offset = info.planes[plane].offset;
    layouts[plane].rowPitch = info.planes[plane].stride;
  }
  VkImageDrmFormatModifierExplicitCreateInfoEXT modifier{
      VK_STRUCTURE_TYPE_IMAGE_DRM_FORMAT_MODIFIER_EXPLICIT_CREATE_INFO_EXT};
  modifier.drmFormatModifier = info.modifier;
  modifier.drmFormatModifierPlaneCount = static_cast<uint32_t>(planes);
  modifier.pPlaneLayouts = layouts.data();
  VkExternalMemoryImageCreateInfo external{
      VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_IMAGE_CREATE_INFO};
  external.pNext = &modifier;
  external.handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT;
  out->width = static_cast<uint32_t>(info.extra.coded_size.width);
  out->height = static_cast<uint32_t>(info.extra.coded_size.height);
  const VkImageCreateInfo image =
      ImageInfo(&external, format, out->width, out->height,
                VK_IMAGE_USAGE_TRANSFER_SRC_BIT);
  if (vulkan->vkCreateImage(vulkan->device, &image, nullptr, &out->image) !=
      VK_SUCCESS) {
    return false;
  }
  const int fd = fcntl(info.planes[0].fd, F_DUPFD_CLOEXEC, 0);
  VkMemoryFdPropertiesKHR properties{
      VK_STRUCTURE_TYPE_MEMORY_FD_PROPERTIES_KHR};
  if (fd < 0 ||
      vulkan->vkGetMemoryFdPropertiesKHR(
          vulkan->device, VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT, fd,
          &properties) != VK_SUCCESS) {
    if (fd >= 0) {
      close(fd);
    }
    DestroyDmabuf(vulkan, out);
    return false;
  }
  VkImportMemoryFdInfoKHR import{VK_STRUCTURE_TYPE_IMPORT_MEMORY_FD_INFO_KHR};
  import.handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT;
  import.fd = fd;
  if (!AllocateDedicated(vulkan, out, &import, properties.memoryTypeBits, 0)) {
    if (!out->memory) {
      close(fd);
    }
    DestroyDmabuf(vulkan, out);
    return false;
  }
  out->modifier = info.modifier;
  return true;
}

bool CreateExportableDmabuf(VulkanContext* vulkan,
                            SharedFormat format,
                            uint32_t width,
                            uint32_t height,
                            DmabufImage* out) {
  const auto& modifiers =
      vulkan->shared_modifiers[format == SharedFormat::kBgra ? 0 : 1];
  if (modifiers.empty()) {
    return false;
  }
  VkImageDrmFormatModifierListCreateInfoEXT list{
      VK_STRUCTURE_TYPE_IMAGE_DRM_FORMAT_MODIFIER_LIST_CREATE_INFO_EXT};
  list.drmFormatModifierCount = static_cast<uint32_t>(modifiers.size());
  list.pDrmFormatModifiers = modifiers.data();
  VkExternalMemoryImageCreateInfo external{
      VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_IMAGE_CREATE_INFO};
  external.pNext = &list;
  external.handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT;
  const VkImageCreateInfo image =
      ImageInfo(&external, UnormFormat(format), width, height,
                VK_IMAGE_USAGE_TRANSFER_DST_BIT);
  out->width = width;
  out->height = height;
  if (vulkan->vkCreateImage(vulkan->device, &image, nullptr, &out->image) !=
      VK_SUCCESS) {
    return false;
  }
  VkExportMemoryAllocateInfo exported{
      VK_STRUCTURE_TYPE_EXPORT_MEMORY_ALLOCATE_INFO};
  exported.handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT;
  VkImageDrmFormatModifierPropertiesEXT modifier{
      VK_STRUCTURE_TYPE_IMAGE_DRM_FORMAT_MODIFIER_PROPERTIES_EXT};
  VkMemoryGetFdInfoKHR get_fd{VK_STRUCTURE_TYPE_MEMORY_GET_FD_INFO_KHR};
  get_fd.handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT;
  if (!AllocateDedicated(vulkan, out, &exported, ~0u,
                         VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT) ||
      vulkan->vkGetImageDrmFormatModifierPropertiesEXT(
          vulkan->device, out->image, &modifier) != VK_SUCCESS) {
    DestroyDmabuf(vulkan, out);
    return false;
  }
  get_fd.memory = out->memory;
  if (vulkan->vkGetMemoryFdKHR(vulkan->device, &get_fd, &out->fd) !=
      VK_SUCCESS) {
    out->fd = -1;
    DestroyDmabuf(vulkan, out);
    return false;
  }
  const VkImageSubresource plane{VK_IMAGE_ASPECT_MEMORY_PLANE_0_BIT_EXT, 0, 0};
  VkSubresourceLayout layout{};
  vulkan->vkGetImageSubresourceLayout(vulkan->device, out->image, &plane,
                                      &layout);
  out->modifier = modifier.drmFormatModifier;
  out->stride = static_cast<uint32_t>(layout.rowPitch);
  out->offset = static_cast<uint32_t>(layout.offset);
  return true;
}

void DestroyDmabuf(VulkanContext* vulkan, DmabufImage* image) {
  if (image->fd >= 0) {
    close(image->fd);
  }
  if (image->image) {
    vulkan->vkDestroyImage(vulkan->device, image->image, nullptr);
  }
  if (image->memory) {
    vulkan->vkFreeMemory(vulkan->device, image->memory, nullptr);
  }
  *image = DmabufImage{};
}

}  // namespace sabine_osr
