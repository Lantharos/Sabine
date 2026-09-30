#include "osr/accelerated/linux/dmabuf_copy.h"

#include <linux/dma-buf.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>
#include <cstdio>
#include <map>

#include "osr/accelerated/linux/dmabuf_image.h"

namespace sabine_osr {
namespace {

constexpr uint32_t kSlotsPerSurface = 4;
constexpr size_t kSourcesPerSurface = 6;
constexpr uint64_t kCopyTimeoutNs = 1000000000;

struct OwnedSlot {
  DmabufImage image;
  uint64_t resource_id = 0;
  SharedFormat format = SharedFormat::kBgra;
  bool in_use = false;
  uint64_t token = 0;
};

struct Source {
  dev_t device = 0;
  ino_t inode = 0;
  uint64_t modifier = 0;
  VkFormat format = VK_FORMAT_UNDEFINED;
  uint64_t last_use = 0;
  DmabufImage image;
};

std::map<std::string, OwnedSlot> g_slots;
std::map<std::string, uint32_t> g_next_slot;
std::map<std::string, std::vector<Source>> g_sources;
uint64_t g_next_token = 1;
uint64_t g_next_resource_id = 1;
uint64_t g_use = 0;

bool Matches(const Source& source,
             const struct stat& buffer,
             const CefAcceleratedPaintInfo& info,
             VkFormat format) {
  return source.device == buffer.st_dev && source.inode == buffer.st_ino &&
         source.modifier == info.modifier && source.format == format &&
         source.image.width ==
             static_cast<uint32_t>(info.extra.coded_size.width) &&
         source.image.height ==
             static_cast<uint32_t>(info.extra.coded_size.height);
}

Source* SourceFor(VulkanContext* vulkan,
                  const std::string& key,
                  const CefAcceleratedPaintInfo& info,
                  VkFormat format) {
  struct stat buffer{};
  if (fstat(info.planes[0].fd, &buffer) != 0) {
    return nullptr;
  }
  auto& sources = g_sources[key];
  for (auto& source : sources) {
    if (Matches(source, buffer, info, format)) {
      source.last_use = ++g_use;
      return &source;
    }
  }
  const auto stale =
      std::remove_if(sources.begin(), sources.end(), [&](Source& source) {
        const bool outdated =
            source.image.width !=
                static_cast<uint32_t>(info.extra.coded_size.width) ||
            source.image.height !=
                static_cast<uint32_t>(info.extra.coded_size.height);
        if (outdated) {
          DestroyDmabuf(vulkan, &source.image);
        }
        return outdated;
      });
  sources.erase(stale, sources.end());
  if (sources.size() >= kSourcesPerSurface) {
    auto oldest = std::min_element(sources.begin(), sources.end(),
                                   [](const Source& a, const Source& b) {
                                     return a.last_use < b.last_use;
                                   });
    DestroyDmabuf(vulkan, &oldest->image);
    sources.erase(oldest);
  }
  Source source;
  source.device = buffer.st_dev;
  source.inode = buffer.st_ino;
  source.modifier = info.modifier;
  source.format = format;
  source.last_use = ++g_use;
  if (!ImportDmabuf(vulkan, info, format, &source.image)) {
    std::fprintf(stderr,
                 "Sabine CEF: could not import Chromium's dma-buf (modifier "
                 "0x%llx)\n",
                 static_cast<unsigned long long>(info.modifier));
    return nullptr;
  }
  sources.push_back(source);
  return &sources.back();
}

bool WaitForProducer(VulkanContext* vulkan, int fd) {
  if (!vulkan->producer_done) {
    return false;
  }
  dma_buf_export_sync_file request{DMA_BUF_SYNC_READ, -1};
  if (ioctl(fd, DMA_BUF_IOCTL_EXPORT_SYNC_FILE, &request) != 0) {
    return false;
  }
  VkImportSemaphoreFdInfoKHR import{
      VK_STRUCTURE_TYPE_IMPORT_SEMAPHORE_FD_INFO_KHR};
  import.semaphore = vulkan->producer_done;
  import.flags = VK_SEMAPHORE_IMPORT_TEMPORARY_BIT;
  import.handleType = VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_SYNC_FD_BIT;
  import.fd = request.fd;
  if (vulkan->vkImportSemaphoreFdKHR(vulkan->device, &import) != VK_SUCCESS) {
    close(request.fd);
    return false;
  }
  return true;
}

VkImageMemoryBarrier Transfer(VkImage image,
                              VkAccessFlags from_access,
                              VkAccessFlags to_access,
                              VkImageLayout from_layout,
                              VkImageLayout to_layout,
                              uint32_t from_family,
                              uint32_t to_family) {
  VkImageMemoryBarrier barrier{VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER};
  barrier.srcAccessMask = from_access;
  barrier.dstAccessMask = to_access;
  barrier.oldLayout = from_layout;
  barrier.newLayout = to_layout;
  barrier.srcQueueFamilyIndex = from_family;
  barrier.dstQueueFamilyIndex = to_family;
  barrier.image = image;
  barrier.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
  return barrier;
}

bool Copy(VulkanContext* vulkan, VkImage source, DmabufImage* target, int fd) {
  const uint32_t ours = vulkan->queue_family;
  const uint32_t external = vulkan->external_queue_family;
  VkCommandBufferBeginInfo begin{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
  begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
  vulkan->vkResetCommandBuffer(vulkan->commands, 0);
  vulkan->vkBeginCommandBuffer(vulkan->commands, &begin);
  const VkImageMemoryBarrier acquire[] = {
      Transfer(source, 0, VK_ACCESS_TRANSFER_READ_BIT, VK_IMAGE_LAYOUT_GENERAL,
               VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, external, ours),
      Transfer(target->image, 0, VK_ACCESS_TRANSFER_WRITE_BIT,
               VK_IMAGE_LAYOUT_UNDEFINED, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
               external, ours),
  };
  vulkan->vkCmdPipelineBarrier(
      vulkan->commands, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT,
      VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, nullptr, 0, nullptr, 2, acquire);
  VkImageCopy region{};
  region.srcSubresource = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 0, 1};
  region.dstSubresource = region.srcSubresource;
  region.extent = {target->width, target->height, 1};
  vulkan->vkCmdCopyImage(vulkan->commands, source,
                         VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, target->image,
                         VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, 1, &region);
  const VkImageMemoryBarrier release[] = {
      Transfer(source, VK_ACCESS_TRANSFER_READ_BIT, 0,
               VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, VK_IMAGE_LAYOUT_GENERAL,
               ours, external),
      Transfer(target->image, VK_ACCESS_TRANSFER_WRITE_BIT, 0,
               VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, VK_IMAGE_LAYOUT_GENERAL,
               ours, external),
  };
  vulkan->vkCmdPipelineBarrier(vulkan->commands, VK_PIPELINE_STAGE_TRANSFER_BIT,
                               VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT, 0, 0,
                               nullptr, 0, nullptr, 2, release);
  vulkan->vkEndCommandBuffer(vulkan->commands);

  const VkPipelineStageFlags wait_stage = VK_PIPELINE_STAGE_TRANSFER_BIT;
  VkSubmitInfo submit{VK_STRUCTURE_TYPE_SUBMIT_INFO};
  if (WaitForProducer(vulkan, fd)) {
    submit.waitSemaphoreCount = 1;
    submit.pWaitSemaphores = &vulkan->producer_done;
    submit.pWaitDstStageMask = &wait_stage;
  }
  submit.commandBufferCount = 1;
  submit.pCommandBuffers = &vulkan->commands;
  const bool completed =
      vulkan->vkQueueSubmit(vulkan->queue, 1, &submit, vulkan->fence) ==
          VK_SUCCESS &&
      vulkan->vkWaitForFences(vulkan->device, 1, &vulkan->fence, VK_TRUE,
                              kCopyTimeoutNs) == VK_SUCCESS;
  vulkan->vkResetFences(vulkan->device, 1, &vulkan->fence);
  return completed;
}

bool EnsureSlot(VulkanContext* vulkan,
                OwnedSlot* slot,
                SharedFormat format,
                uint32_t width,
                uint32_t height,
                std::vector<uint64_t>* retired) {
  if (slot->image.image && slot->format == format &&
      slot->image.width == width && slot->image.height == height) {
    return true;
  }
  if (slot->resource_id != 0) {
    retired->push_back(slot->resource_id);
  }
  DestroyDmabuf(vulkan, &slot->image);
  *slot = OwnedSlot{};
  if (!CreateExportableDmabuf(vulkan, format, width, height, &slot->image)) {
    std::fprintf(stderr, "Sabine CEF: could not create a shared dma-buf\n");
    return false;
  }
  slot->format = format;
  slot->resource_id = g_next_resource_id++;
  return true;
}

}  // namespace

bool AcceleratedDmabufAvailable() {
  return SharedVulkanContext() != nullptr;
}

DmabufCopy CopyAcceleratedDmabufFrame(const std::string& slot_key,
                                      const CefAcceleratedPaintInfo& info,
                                      AccelDmabufCopiedFrame* out) {
  VulkanContext* vulkan = SharedVulkanContext();
  if (!vulkan || info.plane_count < 1) {
    return DmabufCopy::kFailed;
  }
  const SharedFormat format = info.format == CEF_COLOR_TYPE_BGRA_8888
                                  ? SharedFormat::kBgra
                                  : SharedFormat::kRgba;
  const uint32_t first_slot = g_next_slot[slot_key]++ % kSlotsPerSurface;
  for (uint32_t offset = 0; offset < kSlotsPerSurface; ++offset) {
    const uint32_t slot_index = (first_slot + offset) % kSlotsPerSurface;
    OwnedSlot& slot = g_slots[slot_key + "#" + std::to_string(slot_index)];
    if (slot.in_use) {
      continue;
    }
    Source* source = SourceFor(vulkan, slot_key, info, UnormFormat(format));
    if (!source ||
        !EnsureSlot(vulkan, &slot, format, source->image.width,
                    source->image.height, &out->retired_resource_ids) ||
        !Copy(vulkan, source->image.image, &slot.image, info.planes[0].fd)) {
      return DmabufCopy::kFailed;
    }
    slot.in_use = true;
    slot.token = g_next_token++;
    out->fd = slot.image.fd;
    out->resource_id = slot.resource_id;
    out->slot_index = slot_index;
    out->slot_token = slot.token;
    out->width = slot.image.width;
    out->height = slot.image.height;
    out->modifier = slot.image.modifier;
    out->stride = slot.image.stride;
    out->offset = slot.image.offset;
    return DmabufCopy::kCopied;
  }
  return DmabufCopy::kSlotsBusy;
}

void ReleaseAcceleratedDmabufFrame(uint64_t slot_token) {
  for (auto& [key, slot] : g_slots) {
    (void)key;
    if (slot.in_use && slot.token == slot_token) {
      slot.in_use = false;
      slot.token = 0;
      return;
    }
  }
}

std::vector<uint64_t> RetireAcceleratedDmabufBrowser(int browser_id) {
  VulkanContext* vulkan = SharedVulkanContext();
  const std::string prefix = std::to_string(browser_id) + "/";
  std::vector<uint64_t> retired;
  for (auto it = g_slots.begin(); it != g_slots.end();) {
    if (it->first.rfind(prefix, 0) == 0) {
      if (it->second.resource_id != 0) {
        retired.push_back(it->second.resource_id);
      }
      DestroyDmabuf(vulkan, &it->second.image);
      it = g_slots.erase(it);
    } else {
      ++it;
    }
  }
  for (auto it = g_sources.begin(); it != g_sources.end();) {
    if (it->first.rfind(prefix, 0) == 0) {
      for (auto& source : it->second) {
        DestroyDmabuf(vulkan, &source.image);
      }
      it = g_sources.erase(it);
    } else {
      ++it;
    }
  }
  for (auto it = g_next_slot.begin(); it != g_next_slot.end();) {
    if (it->first.rfind(prefix, 0) == 0) {
      it = g_next_slot.erase(it);
    } else {
      ++it;
    }
  }
  return retired;
}

}  // namespace sabine_osr
