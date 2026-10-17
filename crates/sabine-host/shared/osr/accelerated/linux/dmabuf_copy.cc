#include "osr/accelerated/linux/dmabuf_copy.h"

#include <linux/dma-buf.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <cstdio>
#include <map>
#include <optional>

#include "osr/accelerated/linux/dmabuf_image.h"

namespace sabine_osr {
namespace {

constexpr uint32_t kSlotsPerSurface = 4;
constexpr size_t kSourcesPerSurface = 6;
constexpr uint64_t kCopyTimeoutNs = 1000000000;
// While a graphics driver restarts, no device may be available for a few
// seconds, so the copy device keeps being reopened before it gives up.
constexpr std::chrono::seconds kReopenTimeout{20};

struct OwnedSlot {
  DmabufImage image;
  uint64_t resource_id = 0;
  SharedFormat format = SharedFormat::kBgra;
  bool in_use = false;
  bool filled = false;
  uint64_t token = 0;
  PixelRegion stale;
  // CEF passes no fence with a frame (chromiumembedded/cef#4313), and drivers
  // that attach none to the dma-buf, such as NVIDIA's, let a copy start
  // before Chromium finished drawing. The pixels a copy took for new damage
  // are copied again the next time the slot receives a frame.
  PixelRegion unsettled;
};

struct Source {
  dev_t device = 0;
  ino_t inode = 0;
  uint64_t modifier = 0;
  VkFormat format = VK_FORMAT_UNDEFINED;
  uint64_t last_use = 0;
  DmabufImage image;
};

struct SurfaceSlots {
  std::array<OwnedSlot, kSlotsPerSurface> slots;
  uint32_t next = 0;
  std::vector<Source> sources;
  // The use stamp of the source the last frame was copied from.
  uint64_t last_source = 0;
  SharedFormat last_format = SharedFormat::kBgra;
};

std::map<AcceleratedSurfaceKey, SurfaceSlots> g_surfaces;
uint64_t g_next_token = 1;
uint64_t g_next_resource_id = 1;
uint64_t g_use = 0;
std::optional<std::chrono::steady_clock::time_point> g_device_lost_at;

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
                  std::vector<Source>& sources,
                  const CefAcceleratedPaintInfo& info,
                  VkFormat format) {
  struct stat buffer{};
  if (fstat(info.planes[0].fd, &buffer) != 0) {
    return nullptr;
  }
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

bool Copy(VulkanContext* vulkan,
          VkImage source,
          OwnedSlot* slot,
          const PixelRegion& region,
          std::optional<int> drawing_fd) {
  DmabufImage* target = &slot->image;
  const uint32_t ours = vulkan->queue_family;
  const uint32_t external = vulkan->external_queue_family;
  VkCommandBufferBeginInfo begin{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
  begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
  vulkan->vkResetCommandBuffer(vulkan->commands, 0);
  vulkan->vkBeginCommandBuffer(vulkan->commands, &begin);
  const VkImageMemoryBarrier acquire[] = {
      Transfer(source, 0, VK_ACCESS_TRANSFER_READ_BIT, VK_IMAGE_LAYOUT_GENERAL,
               VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, external, ours),
      Transfer(
          target->image, 0, VK_ACCESS_TRANSFER_WRITE_BIT,
          slot->filled ? VK_IMAGE_LAYOUT_GENERAL : VK_IMAGE_LAYOUT_UNDEFINED,
          VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, external, ours),
  };
  vulkan->vkCmdPipelineBarrier(
      vulkan->commands, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT,
      VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, nullptr, 0, nullptr, 2, acquire);
  VkImageCopy copy{};
  copy.srcSubresource = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 0, 1};
  copy.dstSubresource = copy.srcSubresource;
  copy.srcOffset = {region.x, region.y, 0};
  copy.dstOffset = copy.srcOffset;
  copy.extent = {static_cast<uint32_t>(region.width),
                 static_cast<uint32_t>(region.height), 1};
  vulkan->vkCmdCopyImage(vulkan->commands, source,
                         VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, target->image,
                         VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, 1, &copy);
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
  if (drawing_fd && WaitForProducer(vulkan, *drawing_fd)) {
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
  slot->stale =
      PixelRegion::Whole(static_cast<int>(width), static_cast<int>(height));
  return true;
}

// A frame copied as it arrives may still be drawing, so `drawing_fd` names
// the dma-buf whose fences the copy waits for; a finished frame has none.
bool CopyIntoSlot(VulkanContext* vulkan,
                  const Source& source,
                  SharedFormat format,
                  std::optional<int> drawing_fd,
                  OwnedSlot* slot,
                  AccelDmabufCopiedFrame* out) {
  const uint32_t width = source.image.width;
  const uint32_t height = source.image.height;
  if (!EnsureSlot(vulkan, slot, format, width, height,
                  &out->retired_resource_ids)) {
    return false;
  }
  const int surface_width = static_cast<int>(width);
  const int surface_height = static_cast<int>(height);
  const PixelRegion changed = slot->stale.Within(surface_width, surface_height);
  PixelRegion region = changed;
  region.Unite(slot->unsettled.Within(surface_width, surface_height));
  if (!region.empty()) {
    if (!Copy(vulkan, source.image.image, slot, region, drawing_fd)) {
      return false;
    }
    slot->filled = true;
  }
  slot->stale = {};
  slot->unsettled = drawing_fd ? changed : PixelRegion{};
  slot->in_use = true;
  slot->token = g_next_token++;
  out->fd = slot->image.fd;
  out->resource_id = slot->resource_id;
  out->slot_token = slot->token;
  out->width = width;
  out->height = height;
  out->modifier = slot->image.modifier;
  out->stride = slot->image.stride;
  out->offset = slot->image.offset;
  return true;
}

std::optional<uint32_t> FreeSlot(SurfaceSlots& slots) {
  const uint32_t first_slot = slots.next++ % kSlotsPerSurface;
  for (uint32_t offset = 0; offset < kSlotsPerSurface; ++offset) {
    const uint32_t slot_index = (first_slot + offset) % kSlotsPerSurface;
    if (!slots.slots[slot_index].in_use) {
      return slot_index;
    }
  }
  return std::nullopt;
}

DmabufCopy CopyFrame(VulkanContext* vulkan,
                     const AcceleratedSurfaceKey& surface,
                     const CefAcceleratedPaintInfo& info,
                     const PixelRegion& damage,
                     AccelDmabufCopiedFrame* out) {
  const SharedFormat format = info.format == CEF_COLOR_TYPE_BGRA_8888
                                  ? SharedFormat::kBgra
                                  : SharedFormat::kRgba;
  SurfaceSlots& slots = g_surfaces[surface];
  for (OwnedSlot& slot : slots.slots) {
    slot.stale.Unite(damage);
  }
  const std::optional<uint32_t> slot_index = FreeSlot(slots);
  if (!slot_index) {
    return DmabufCopy::kSlotsBusy;
  }
  Source* source = SourceFor(vulkan, slots.sources, info, UnormFormat(format));
  if (!source || !CopyIntoSlot(vulkan, *source, format, info.planes[0].fd,
                               &slots.slots[*slot_index], out)) {
    return DmabufCopy::kFailed;
  }
  slots.last_source = source->last_use;
  slots.last_format = format;
  out->slot_index = *slot_index;
  return DmabufCopy::kCopied;
}

void ForgetSurfaces(VulkanContext* vulkan, std::vector<uint64_t>* retired) {
  for (auto& [surface, slots] : g_surfaces) {
    for (OwnedSlot& slot : slots.slots) {
      if (slot.resource_id != 0) {
        retired->push_back(slot.resource_id);
      }
      DestroyDmabuf(vulkan, &slot.image);
    }
    for (Source& source : slots.sources) {
      DestroyDmabuf(vulkan, &source.image);
    }
  }
  g_surfaces.clear();
}

DmabufCopy ReopenAndCopy(const AcceleratedSurfaceKey& surface,
                         const CefAcceleratedPaintInfo& info,
                         const PixelRegion& damage,
                         AccelDmabufCopiedFrame* out) {
  VulkanContext* vulkan = ReopenSharedVulkanContext();
  if (!vulkan) {
    const bool waiting =
        std::chrono::steady_clock::now() - *g_device_lost_at < kReopenTimeout;
    return waiting ? DmabufCopy::kReopening : DmabufCopy::kFailed;
  }
  g_device_lost_at.reset();
  out->device_reopened = true;
  std::fprintf(stderr, "Sabine CEF: the copy device is back\n");
  std::fflush(stderr);
  return CopyFrame(vulkan, surface, info, damage, out);
}

}  // namespace

bool AcceleratedDmabufAvailable() {
  return SharedVulkanContext() != nullptr;
}

DmabufCopy CopyAcceleratedDmabufFrame(const AcceleratedSurfaceKey& surface,
                                      const CefAcceleratedPaintInfo& info,
                                      const PixelRegion& damage,
                                      AccelDmabufCopiedFrame* out) {
  if (info.plane_count < 1) {
    return DmabufCopy::kFailed;
  }
  if (g_device_lost_at) {
    return ReopenAndCopy(surface, info, damage, out);
  }
  VulkanContext* vulkan = SharedVulkanContext();
  if (!vulkan) {
    return DmabufCopy::kFailed;
  }
  const DmabufCopy result = CopyFrame(vulkan, surface, info, damage, out);
  if (result != DmabufCopy::kFailed || !VulkanDeviceLost(vulkan)) {
    return result;
  }
  std::fprintf(stderr,
               "Sabine CEF: the graphics device was lost; reopening the copy "
               "device\n");
  std::fflush(stderr);
  ForgetSurfaces(vulkan, &out->retired_resource_ids);
  g_device_lost_at = std::chrono::steady_clock::now();
  return ReopenAndCopy(surface, info, damage, out);
}

DmabufCopy CopyFinishedDmabufFrame(const AcceleratedSurfaceKey& surface,
                                   AccelDmabufCopiedFrame* out) {
  const auto found = g_surfaces.find(surface);
  VulkanContext* vulkan = SharedVulkanContext();
  if (found == g_surfaces.end() || g_device_lost_at || !vulkan) {
    return DmabufCopy::kFailed;
  }
  SurfaceSlots& slots = found->second;
  const auto source = std::find_if(
      slots.sources.begin(), slots.sources.end(), [&](const Source& candidate) {
        return candidate.last_use == slots.last_source;
      });
  if (source == slots.sources.end()) {
    return DmabufCopy::kFailed;
  }
  const std::optional<uint32_t> slot_index = FreeSlot(slots);
  if (!slot_index) {
    return DmabufCopy::kSlotsBusy;
  }
  OwnedSlot& slot = slots.slots[*slot_index];
  slot.stale = PixelRegion::Whole(static_cast<int>(source->image.width),
                                  static_cast<int>(source->image.height));
  if (!CopyIntoSlot(vulkan, *source, slots.last_format, std::nullopt, &slot,
                    out)) {
    return DmabufCopy::kFailed;
  }
  out->slot_index = *slot_index;
  return DmabufCopy::kCopied;
}

void ReleaseAcceleratedDmabufFrame(uint64_t slot_token) {
  for (auto& [surface, slots] : g_surfaces) {
    for (OwnedSlot& slot : slots.slots) {
      if (slot.in_use && slot.token == slot_token) {
        slot.in_use = false;
        slot.token = 0;
        return;
      }
    }
  }
}

std::vector<uint64_t> RetireAcceleratedDmabufBrowser(int browser_id) {
  VulkanContext* vulkan = SharedVulkanContext();
  std::vector<uint64_t> retired;
  for (auto it = g_surfaces.begin(); it != g_surfaces.end();) {
    if (it->first.browser_id != browser_id) {
      ++it;
      continue;
    }
    for (OwnedSlot& slot : it->second.slots) {
      if (slot.resource_id != 0) {
        retired.push_back(slot.resource_id);
      }
      DestroyDmabuf(vulkan, &slot.image);
    }
    for (Source& source : it->second.sources) {
      DestroyDmabuf(vulkan, &source.image);
    }
    it = g_surfaces.erase(it);
  }
  return retired;
}

}  // namespace sabine_osr
