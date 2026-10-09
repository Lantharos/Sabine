#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_LINUX_VULKAN_CONTEXT_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_LINUX_VULKAN_CONTEXT_H_

#define VK_NO_PROTOTYPES
#include <vulkan/vulkan.h>

#include <cstdint>
#include <vector>

namespace sabine_osr {

#define SABINE_VULKAN_DEVICE_FUNCTIONS(X) \
  X(vkGetDeviceQueue)                     \
  X(vkCreateCommandPool)                  \
  X(vkAllocateCommandBuffers)             \
  X(vkResetCommandBuffer)                 \
  X(vkBeginCommandBuffer)                 \
  X(vkEndCommandBuffer)                   \
  X(vkCmdPipelineBarrier)                 \
  X(vkCmdCopyImage)                       \
  X(vkQueueSubmit)                        \
  X(vkCreateFence)                        \
  X(vkResetFences)                        \
  X(vkWaitForFences)                      \
  X(vkCreateSemaphore)                    \
  X(vkCreateImage)                        \
  X(vkDestroyImage)                       \
  X(vkGetImageMemoryRequirements)         \
  X(vkGetImageSubresourceLayout)          \
  X(vkAllocateMemory)                     \
  X(vkFreeMemory)                         \
  X(vkBindImageMemory)                    \
  X(vkGetMemoryFdKHR)                     \
  X(vkGetMemoryFdPropertiesKHR)           \
  X(vkGetImageDrmFormatModifierPropertiesEXT)

#define SABINE_VULKAN_OPTIONAL_DEVICE_FUNCTIONS(X) X(vkImportSemaphoreFdKHR)

struct VulkanContext {
  VkInstance instance = VK_NULL_HANDLE;
  VkPhysicalDevice physical_device = VK_NULL_HANDLE;
  VkDevice device = VK_NULL_HANDLE;
  VkQueue queue = VK_NULL_HANDLE;
  uint32_t queue_family = 0;
  uint32_t external_queue_family = VK_QUEUE_FAMILY_EXTERNAL;
  VkCommandPool command_pool = VK_NULL_HANDLE;
  VkCommandBuffer commands = VK_NULL_HANDLE;
  VkFence fence = VK_NULL_HANDLE;
  VkSemaphore producer_done = VK_NULL_HANDLE;
  VkPhysicalDeviceMemoryProperties memory{};
  std::vector<uint64_t> shared_modifiers[2];

#define SABINE_VULKAN_MEMBER(name) PFN_##name name = nullptr;
  SABINE_VULKAN_DEVICE_FUNCTIONS(SABINE_VULKAN_MEMBER)
  SABINE_VULKAN_OPTIONAL_DEVICE_FUNCTIONS(SABINE_VULKAN_MEMBER)
#undef SABINE_VULKAN_MEMBER
};

enum class SharedFormat { kBgra, kRgba };

VkFormat UnormFormat(SharedFormat format);

// Opens the Vulkan device used to copy Chromium's frames into Sabine-owned
// dma-bufs, once per process. Returns null when this machine cannot share
// dma-bufs through Vulkan.
VulkanContext* SharedVulkanContext();

}  // namespace sabine_osr

#endif
