#include "osr/accelerated/linux/vulkan_context.h"

#include <dlfcn.h>

#include <array>
#include <cstdio>
#include <cstring>
#include <memory>
#include <optional>

namespace sabine_osr {
namespace {

constexpr std::array<const char*, 3> kRequiredExtensions = {
    VK_KHR_EXTERNAL_MEMORY_FD_EXTENSION_NAME,
    VK_EXT_EXTERNAL_MEMORY_DMA_BUF_EXTENSION_NAME,
    VK_EXT_IMAGE_DRM_FORMAT_MODIFIER_EXTENSION_NAME,
};
constexpr VkFormatFeatureFlags kSharedFeatures =
    VK_FORMAT_FEATURE_TRANSFER_DST_BIT | VK_FORMAT_FEATURE_SAMPLED_IMAGE_BIT |
    VK_FORMAT_FEATURE_SAMPLED_IMAGE_FILTER_LINEAR_BIT;

struct InstanceFunctions {
  PFN_vkDestroyInstance vkDestroyInstance = nullptr;
  PFN_vkEnumeratePhysicalDevices vkEnumeratePhysicalDevices = nullptr;
  PFN_vkGetPhysicalDeviceProperties vkGetPhysicalDeviceProperties = nullptr;
  PFN_vkGetPhysicalDeviceQueueFamilyProperties
      vkGetPhysicalDeviceQueueFamilyProperties = nullptr;
  PFN_vkEnumerateDeviceExtensionProperties
      vkEnumerateDeviceExtensionProperties = nullptr;
  PFN_vkGetPhysicalDeviceMemoryProperties vkGetPhysicalDeviceMemoryProperties =
      nullptr;
  PFN_vkGetPhysicalDeviceFormatProperties2
      vkGetPhysicalDeviceFormatProperties2 = nullptr;
  PFN_vkGetPhysicalDeviceImageFormatProperties2
      vkGetPhysicalDeviceImageFormatProperties2 = nullptr;
  PFN_vkGetPhysicalDeviceExternalSemaphoreProperties
      vkGetPhysicalDeviceExternalSemaphoreProperties = nullptr;
  PFN_vkCreateDevice vkCreateDevice = nullptr;
  PFN_vkGetDeviceProcAddr vkGetDeviceProcAddr = nullptr;
};

struct Candidate {
  VkPhysicalDevice device = VK_NULL_HANDLE;
  uint32_t queue_family = 0;
  bool foreign_queue = false;
  bool semaphore_fd = false;
  std::vector<uint64_t> modifiers[2];
};

VkFormat SrgbFormat(SharedFormat format) {
  return format == SharedFormat::kBgra ? VK_FORMAT_B8G8R8A8_SRGB
                                       : VK_FORMAT_R8G8B8A8_SRGB;
}

int DeviceRank(VkPhysicalDeviceType type) {
  switch (type) {
    case VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU:
      return 0;
    case VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU:
      return 1;
    case VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU:
      return 2;
    default:
      return -1;
  }
}

bool HasExtension(const std::vector<VkExtensionProperties>& extensions,
                  const char* name) {
  for (const auto& extension : extensions) {
    if (std::strcmp(extension.extensionName, name) == 0) {
      return true;
    }
  }
  return false;
}

bool SupportsDmabuf(const InstanceFunctions& vk,
                    VkPhysicalDevice device,
                    VkFormat format,
                    uint64_t modifier,
                    VkImageUsageFlags usage,
                    VkExternalMemoryFeatureFlags required) {
  VkPhysicalDeviceImageDrmFormatModifierInfoEXT modifier_info{
      VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_IMAGE_DRM_FORMAT_MODIFIER_INFO_EXT};
  modifier_info.drmFormatModifier = modifier;
  modifier_info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
  VkPhysicalDeviceExternalImageFormatInfo external_info{
      VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_EXTERNAL_IMAGE_FORMAT_INFO};
  external_info.pNext = &modifier_info;
  external_info.handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT;
  VkPhysicalDeviceImageFormatInfo2 info{
      VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_IMAGE_FORMAT_INFO_2};
  info.pNext = &external_info;
  info.format = format;
  info.type = VK_IMAGE_TYPE_2D;
  info.tiling = VK_IMAGE_TILING_DRM_FORMAT_MODIFIER_EXT;
  info.usage = usage;
  VkExternalImageFormatProperties external{
      VK_STRUCTURE_TYPE_EXTERNAL_IMAGE_FORMAT_PROPERTIES};
  VkImageFormatProperties2 properties{
      VK_STRUCTURE_TYPE_IMAGE_FORMAT_PROPERTIES_2};
  properties.pNext = &external;
  return vk.vkGetPhysicalDeviceImageFormatProperties2(
             device, &info, &properties) == VK_SUCCESS &&
         (external.externalMemoryProperties.externalMemoryFeatures &
          required) == required;
}

std::vector<uint64_t> SharedModifiers(const InstanceFunctions& vk,
                                      VkPhysicalDevice device,
                                      SharedFormat format) {
  const VkFormat unorm = UnormFormat(format);
  VkDrmFormatModifierPropertiesListEXT list{
      VK_STRUCTURE_TYPE_DRM_FORMAT_MODIFIER_PROPERTIES_LIST_EXT};
  VkFormatProperties2 properties{VK_STRUCTURE_TYPE_FORMAT_PROPERTIES_2};
  properties.pNext = &list;
  vk.vkGetPhysicalDeviceFormatProperties2(device, unorm, &properties);
  std::vector<VkDrmFormatModifierPropertiesEXT> modifiers(
      list.drmFormatModifierCount);
  list.pDrmFormatModifierProperties = modifiers.data();
  vk.vkGetPhysicalDeviceFormatProperties2(device, unorm, &properties);
  modifiers.resize(list.drmFormatModifierCount);

  std::vector<uint64_t> shared;
  for (const auto& modifier : modifiers) {
    if (modifier.drmFormatModifierPlaneCount != 1 ||
        (modifier.drmFormatModifierTilingFeatures & kSharedFeatures) !=
            kSharedFeatures) {
      continue;
    }
    const bool exportable =
        SupportsDmabuf(vk, device, unorm, modifier.drmFormatModifier,
                       VK_IMAGE_USAGE_TRANSFER_DST_BIT,
                       VK_EXTERNAL_MEMORY_FEATURE_EXPORTABLE_BIT);
    const bool importable = SupportsDmabuf(
        vk, device, SrgbFormat(format), modifier.drmFormatModifier,
        VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT,
        VK_EXTERNAL_MEMORY_FEATURE_IMPORTABLE_BIT);
    if (exportable && importable) {
      shared.push_back(modifier.drmFormatModifier);
    }
  }
  return shared;
}

std::optional<Candidate> Inspect(const InstanceFunctions& vk,
                                 VkPhysicalDevice device) {
  uint32_t count = 0;
  vk.vkEnumerateDeviceExtensionProperties(device, nullptr, &count, nullptr);
  std::vector<VkExtensionProperties> extensions(count);
  vk.vkEnumerateDeviceExtensionProperties(device, nullptr, &count,
                                          extensions.data());
  for (const char* required : kRequiredExtensions) {
    if (!HasExtension(extensions, required)) {
      return std::nullopt;
    }
  }
  vk.vkGetPhysicalDeviceQueueFamilyProperties(device, &count, nullptr);
  std::vector<VkQueueFamilyProperties> families(count);
  vk.vkGetPhysicalDeviceQueueFamilyProperties(device, &count, families.data());
  Candidate candidate;
  candidate.device = device;
  candidate.queue_family = count;
  for (uint32_t index = 0; index < count; ++index) {
    if (families[index].queueFlags & VK_QUEUE_GRAPHICS_BIT) {
      candidate.queue_family = index;
      break;
    }
  }
  if (candidate.queue_family == count) {
    return std::nullopt;
  }
  candidate.modifiers[0] = SharedModifiers(vk, device, SharedFormat::kBgra);
  candidate.modifiers[1] = SharedModifiers(vk, device, SharedFormat::kRgba);
  if (candidate.modifiers[0].empty()) {
    return std::nullopt;
  }
  candidate.foreign_queue =
      HasExtension(extensions, VK_EXT_QUEUE_FAMILY_FOREIGN_EXTENSION_NAME);
  if (HasExtension(extensions, VK_KHR_EXTERNAL_SEMAPHORE_FD_EXTENSION_NAME)) {
    VkPhysicalDeviceExternalSemaphoreInfo info{
        VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_EXTERNAL_SEMAPHORE_INFO};
    info.handleType = VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_SYNC_FD_BIT;
    VkExternalSemaphoreProperties properties{
        VK_STRUCTURE_TYPE_EXTERNAL_SEMAPHORE_PROPERTIES};
    vk.vkGetPhysicalDeviceExternalSemaphoreProperties(device, &info,
                                                      &properties);
    candidate.semaphore_fd = properties.externalSemaphoreFeatures &
                             VK_EXTERNAL_SEMAPHORE_FEATURE_IMPORTABLE_BIT;
  }
  return candidate;
}

std::optional<Candidate> ChooseDevice(const InstanceFunctions& vk,
                                      VkInstance instance) {
  uint32_t count = 0;
  vk.vkEnumeratePhysicalDevices(instance, &count, nullptr);
  std::vector<VkPhysicalDevice> devices(count);
  vk.vkEnumeratePhysicalDevices(instance, &count, devices.data());
  std::optional<Candidate> chosen;
  int chosen_rank = -1;
  for (VkPhysicalDevice device : devices) {
    VkPhysicalDeviceProperties properties;
    vk.vkGetPhysicalDeviceProperties(device, &properties);
    const int rank = DeviceRank(properties.deviceType);
    if (rank < 0 || properties.apiVersion < VK_API_VERSION_1_2 ||
        (chosen && rank >= chosen_rank)) {
      continue;
    }
    if (auto candidate = Inspect(vk, device)) {
      chosen = std::move(candidate);
      chosen_rank = rank;
    }
  }
  return chosen;
}

bool CreateDevice(const InstanceFunctions& vk,
                  Candidate& candidate,
                  VulkanContext* context) {
  std::vector<const char*> extensions(kRequiredExtensions.begin(),
                                      kRequiredExtensions.end());
  if (candidate.foreign_queue) {
    extensions.push_back(VK_EXT_QUEUE_FAMILY_FOREIGN_EXTENSION_NAME);
  }
  if (candidate.semaphore_fd) {
    extensions.push_back(VK_KHR_EXTERNAL_SEMAPHORE_FD_EXTENSION_NAME);
  }
  const float priority = 1.0f;
  VkDeviceQueueCreateInfo queue{VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO};
  queue.queueFamilyIndex = candidate.queue_family;
  queue.queueCount = 1;
  queue.pQueuePriorities = &priority;
  VkDeviceCreateInfo info{VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO};
  info.queueCreateInfoCount = 1;
  info.pQueueCreateInfos = &queue;
  info.enabledExtensionCount = static_cast<uint32_t>(extensions.size());
  info.ppEnabledExtensionNames = extensions.data();
  if (vk.vkCreateDevice(candidate.device, &info, nullptr, &context->device) !=
      VK_SUCCESS) {
    return false;
  }
#define SABINE_VULKAN_LOAD(name)                                  \
  context->name = reinterpret_cast<PFN_##name>(                   \
      vk.vkGetDeviceProcAddr(context->device, #name));            \
  if (!context->name) {                                           \
    std::fprintf(stderr, "Sabine CEF: Vulkan lacks %s\n", #name); \
    return false;                                                 \
  }
  SABINE_VULKAN_DEVICE_FUNCTIONS(SABINE_VULKAN_LOAD)
#undef SABINE_VULKAN_LOAD
  if (candidate.semaphore_fd) {
    context->vkImportSemaphoreFdKHR =
        reinterpret_cast<PFN_vkImportSemaphoreFdKHR>(
            vk.vkGetDeviceProcAddr(context->device, "vkImportSemaphoreFdKHR"));
  }

  context->physical_device = candidate.device;
  context->queue_family = candidate.queue_family;
  context->external_queue_family = candidate.foreign_queue
                                       ? VK_QUEUE_FAMILY_FOREIGN_EXT
                                       : VK_QUEUE_FAMILY_EXTERNAL;
  context->shared_modifiers[0] = std::move(candidate.modifiers[0]);
  context->shared_modifiers[1] = std::move(candidate.modifiers[1]);
  vk.vkGetPhysicalDeviceMemoryProperties(candidate.device, &context->memory);
  context->vkGetDeviceQueue(context->device, candidate.queue_family, 0,
                            &context->queue);

  VkCommandPoolCreateInfo pool{VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
  pool.flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;
  pool.queueFamilyIndex = candidate.queue_family;
  VkCommandBufferAllocateInfo buffers{
      VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
  buffers.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
  buffers.commandBufferCount = 1;
  VkFenceCreateInfo fence{VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};
  if (context->vkCreateCommandPool(context->device, &pool, nullptr,
                                   &context->command_pool) != VK_SUCCESS) {
    return false;
  }
  buffers.commandPool = context->command_pool;
  if (context->vkAllocateCommandBuffers(context->device, &buffers,
                                        &context->commands) != VK_SUCCESS ||
      context->vkCreateFence(context->device, &fence, nullptr,
                             &context->fence) != VK_SUCCESS) {
    return false;
  }
  if (context->vkImportSemaphoreFdKHR) {
    VkSemaphoreCreateInfo semaphore{VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO};
    context->vkCreateSemaphore(context->device, &semaphore, nullptr,
                               &context->producer_done);
  }
  return true;
}

VulkanContext* CreateContext() {
  void* library = dlopen("libvulkan.so.1", RTLD_NOW | RTLD_LOCAL);
  if (!library) {
    return nullptr;
  }
  auto get_instance_proc = reinterpret_cast<PFN_vkGetInstanceProcAddr>(
      dlsym(library, "vkGetInstanceProcAddr"));
  auto create_instance =
      get_instance_proc ? reinterpret_cast<PFN_vkCreateInstance>(
                              get_instance_proc(nullptr, "vkCreateInstance"))
                        : nullptr;
  if (!create_instance) {
    return nullptr;
  }
  VkApplicationInfo application{VK_STRUCTURE_TYPE_APPLICATION_INFO};
  application.pApplicationName = "Sabine";
  application.apiVersion = VK_API_VERSION_1_2;
  VkInstanceCreateInfo info{VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO};
  info.pApplicationInfo = &application;
  auto context = std::make_unique<VulkanContext>();
  if (create_instance(&info, nullptr, &context->instance) != VK_SUCCESS) {
    return nullptr;
  }
  InstanceFunctions vk;
#define SABINE_VULKAN_INSTANCE(name)      \
  vk.name = reinterpret_cast<PFN_##name>( \
      get_instance_proc(context->instance, #name));
  SABINE_VULKAN_INSTANCE(vkDestroyInstance)
  SABINE_VULKAN_INSTANCE(vkEnumeratePhysicalDevices)
  SABINE_VULKAN_INSTANCE(vkGetPhysicalDeviceProperties)
  SABINE_VULKAN_INSTANCE(vkGetPhysicalDeviceQueueFamilyProperties)
  SABINE_VULKAN_INSTANCE(vkEnumerateDeviceExtensionProperties)
  SABINE_VULKAN_INSTANCE(vkGetPhysicalDeviceMemoryProperties)
  SABINE_VULKAN_INSTANCE(vkGetPhysicalDeviceFormatProperties2)
  SABINE_VULKAN_INSTANCE(vkGetPhysicalDeviceImageFormatProperties2)
  SABINE_VULKAN_INSTANCE(vkGetPhysicalDeviceExternalSemaphoreProperties)
  SABINE_VULKAN_INSTANCE(vkCreateDevice)
  SABINE_VULKAN_INSTANCE(vkGetDeviceProcAddr)
#undef SABINE_VULKAN_INSTANCE
  auto candidate = ChooseDevice(vk, context->instance);
  if (!candidate) {
    vk.vkDestroyInstance(context->instance, nullptr);
  }
  if (!candidate || !CreateDevice(vk, *candidate, context.get())) {
    std::fprintf(stderr,
                 "Sabine CEF: no Vulkan device can share frames as dma-bufs\n");
    return nullptr;
  }
  return context.release();
}

}  // namespace

VkFormat UnormFormat(SharedFormat format) {
  return format == SharedFormat::kBgra ? VK_FORMAT_B8G8R8A8_UNORM
                                       : VK_FORMAT_R8G8B8A8_UNORM;
}

VulkanContext* SharedVulkanContext() {
  static VulkanContext* context = CreateContext();
  return context;
}

}  // namespace sabine_osr
