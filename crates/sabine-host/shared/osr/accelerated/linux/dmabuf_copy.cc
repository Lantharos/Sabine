#include "osr/accelerated/linux/dmabuf_copy.h"

#include <sys/stat.h>

#include <algorithm>
#include <chrono>
#include <cstdio>
#include <optional>

#include "osr/accelerated/linux/copy_sync.h"
#include "osr/accelerated/linux/dmabuf_image.h"
#include "osr/accelerated/slot_ring.h"

namespace sabine_osr {
namespace {

constexpr size_t kSourcesPerSurface = 6;
// While a graphics driver restarts, no device may be available for a few
// seconds, so the copy device keeps being reopened before it gives up.
constexpr std::chrono::seconds kReopenTimeout{20};

struct OwnedSlot : RingSlot {
  DmabufImage image;
  CopySubmission submission;
  SharedFormat format = SharedFormat::kBgra;
  bool filled = false;
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

struct SurfaceSources {
  std::vector<Source> sources;
  // The use stamp of the source the last frame was copied from.
  uint64_t last_source = 0;
  SharedFormat last_format = SharedFormat::kBgra;
};

using SurfaceSlots = SurfaceRing<OwnedSlot, SurfaceSources>;

SlotRings<OwnedSlot, SurfaceSources> g_surfaces;
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

// Forgets the sources Chromium no longer draws into, after the copies that
// still read them finished.
void Evict(VulkanContext* vulkan,
           std::vector<Source>& sources,
           const CefAcceleratedPaintInfo& info) {
  const auto current =
      std::partition(sources.begin(), sources.end(), [&](const Source& source) {
        return source.image.width ==
                   static_cast<uint32_t>(info.extra.coded_size.width) &&
               source.image.height ==
                   static_cast<uint32_t>(info.extra.coded_size.height);
      });
  if (current == sources.end() && sources.size() < kSourcesPerSurface) {
    return;
  }
  vulkan->vkDeviceWaitIdle(vulkan->device);
  for (auto source = current; source != sources.end(); ++source) {
    DestroyDmabuf(vulkan, &source->image);
  }
  sources.erase(current, sources.end());
  if (sources.size() >= kSourcesPerSurface) {
    auto oldest = std::min_element(sources.begin(), sources.end(),
                                   [](const Source& a, const Source& b) {
                                     return a.last_use < b.last_use;
                                   });
    DestroyDmabuf(vulkan, &oldest->image);
    sources.erase(oldest);
  }
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
  Evict(vulkan, sources, info);
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

void RecordCopy(VulkanContext* vulkan,
                VkImage source,
                OwnedSlot* slot,
                const PixelRegion& region) {
  VkImage target = slot->image.image;
  VkCommandBuffer commands = slot->submission.commands;
  const uint32_t ours = vulkan->queue_family;
  const uint32_t external = vulkan->external_queue_family;
  VkCommandBufferBeginInfo begin{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
  begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
  vulkan->vkResetCommandBuffer(commands, 0);
  vulkan->vkBeginCommandBuffer(commands, &begin);
  const VkImageMemoryBarrier acquire[] = {
      Transfer(source, 0, VK_ACCESS_TRANSFER_READ_BIT, VK_IMAGE_LAYOUT_GENERAL,
               VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, external, ours),
      Transfer(
          target, 0, VK_ACCESS_TRANSFER_WRITE_BIT,
          slot->filled ? VK_IMAGE_LAYOUT_GENERAL : VK_IMAGE_LAYOUT_UNDEFINED,
          VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, external, ours),
  };
  vulkan->vkCmdPipelineBarrier(commands, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT,
                               VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, nullptr, 0,
                               nullptr, 2, acquire);
  VkImageCopy copy{};
  copy.srcSubresource = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 0, 1};
  copy.dstSubresource = copy.srcSubresource;
  copy.srcOffset = {region.x, region.y, 0};
  copy.dstOffset = copy.srcOffset;
  copy.extent = {static_cast<uint32_t>(region.width),
                 static_cast<uint32_t>(region.height), 1};
  vulkan->vkCmdCopyImage(commands, source, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
                         target, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, 1,
                         &copy);
  const VkImageMemoryBarrier release[] = {
      Transfer(source, VK_ACCESS_TRANSFER_READ_BIT, 0,
               VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, VK_IMAGE_LAYOUT_GENERAL,
               ours, external),
      Transfer(target, VK_ACCESS_TRANSFER_WRITE_BIT, 0,
               VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, VK_IMAGE_LAYOUT_GENERAL,
               ours, external),
  };
  vulkan->vkCmdPipelineBarrier(commands, VK_PIPELINE_STAGE_TRANSFER_BIT,
                               VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT, 0, 0,
                               nullptr, 0, nullptr, 2, release);
  vulkan->vkEndCommandBuffer(commands);
}

bool EnsureSlot(VulkanContext* vulkan,
                OwnedSlot* slot,
                SharedFormat format,
                uint32_t width,
                uint32_t height,
                std::vector<uint64_t>* retired) {
  if (!slot->submission.commands &&
      !CreateCopySubmission(vulkan, &slot->submission)) {
    return false;
  }
  if (slot->image.image && slot->format == format &&
      slot->image.width == width && slot->image.height == height) {
    return true;
  }
  if (slot->resource_id != 0) {
    retired->push_back(slot->resource_id);
  }
  DestroyDmabuf(vulkan, &slot->image);
  slot->resource_id = 0;
  slot->filled = false;
  slot->unsettled = {};
  if (!CreateExportableDmabuf(vulkan, format, width, height, &slot->image)) {
    std::fprintf(stderr, "Sabine CEF: could not create a shared dma-buf\n");
    return false;
  }
  slot->format = format;
  slot->resource_id = g_surfaces.NewResourceId();
  slot->stale =
      PixelRegion::Whole(static_cast<int>(width), static_cast<int>(height));
  return true;
}

// A frame copied as it arrives comes with Chromium's dma-buf `source_fd`; a
// settled frame is copied again from a buffer Chromium already took back.
bool CopyIntoSlot(VulkanContext* vulkan,
                  const Source& source,
                  SharedFormat format,
                  std::optional<int> source_fd,
                  OwnedSlot* slot,
                  AccelDmabufCopiedFrame* out) {
  const uint32_t width = source.image.width;
  const uint32_t height = source.image.height;
  if (!FinishCopy(vulkan, &slot->submission) ||
      !EnsureSlot(vulkan, slot, format, width, height,
                  &out->retired_resource_ids)) {
    return false;
  }
  const int surface_width = static_cast<int>(width);
  const int surface_height = static_cast<int>(height);
  const PixelRegion changed = slot->stale.Within(surface_width, surface_height);
  PixelRegion region = changed;
  region.Unite(slot->unsettled.Within(surface_width, surface_height));
  if (!region.empty()) {
    RecordCopy(vulkan, source.image.image, slot, region);
    if (!SubmitCopy(vulkan, &slot->submission, source_fd, slot->image.fd)) {
      return false;
    }
    slot->filled = true;
  }
  slot->unsettled =
      kFramesMayStillBeDrawing && source_fd ? changed : PixelRegion{};
  out->slot_token = g_surfaces.Claim(*slot);
  out->fd = slot->image.fd;
  out->resource_id = slot->resource_id;
  out->width = width;
  out->height = height;
  out->modifier = slot->image.modifier;
  out->stride = slot->image.stride;
  out->offset = slot->image.offset;
  return true;
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
  slots.Damage(damage);
  const std::optional<uint32_t> slot_index = slots.FreeSlot();
  if (!slot_index) {
    return DmabufCopy::kSlotsBusy;
  }
  Source* source =
      SourceFor(vulkan, slots.extra.sources, info, UnormFormat(format));
  if (!source || !CopyIntoSlot(vulkan, *source, format, info.planes[0].fd,
                               &slots.slots[*slot_index], out)) {
    return DmabufCopy::kFailed;
  }
  slots.extra.last_source = source->last_use;
  slots.extra.last_format = format;
  out->slot_index = *slot_index;
  return DmabufCopy::kCopied;
}

void DestroySurface(VulkanContext* vulkan, SurfaceSlots& slots) {
  vulkan->vkDeviceWaitIdle(vulkan->device);
  for (OwnedSlot& slot : slots.slots) {
    DestroyCopySubmission(vulkan, &slot.submission);
    DestroyDmabuf(vulkan, &slot.image);
  }
  for (Source& source : slots.extra.sources) {
    DestroyDmabuf(vulkan, &source.image);
  }
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
  const std::vector<uint64_t> retired = g_surfaces.RetireAll(
      [vulkan](SurfaceSlots& slots) { DestroySurface(vulkan, slots); });
  out->retired_resource_ids.insert(out->retired_resource_ids.end(),
                                   retired.begin(), retired.end());
  g_device_lost_at = std::chrono::steady_clock::now();
  return ReopenAndCopy(surface, info, damage, out);
}

DmabufCopy CopyFinishedDmabufFrame(const AcceleratedSurfaceKey& surface,
                                   AccelDmabufCopiedFrame* out) {
  SurfaceSlots* found = g_surfaces.Find(surface);
  VulkanContext* vulkan = SharedVulkanContext();
  if (!found || g_device_lost_at || !vulkan) {
    return DmabufCopy::kFailed;
  }
  SurfaceSlots& slots = *found;
  const std::vector<Source>& sources = slots.extra.sources;
  const auto source = std::find_if(
      sources.begin(), sources.end(), [&](const Source& candidate) {
        return candidate.last_use == slots.extra.last_source;
      });
  if (source == sources.end()) {
    return DmabufCopy::kFailed;
  }
  const std::optional<uint32_t> slot_index = slots.FreeSlot();
  if (!slot_index) {
    return DmabufCopy::kSlotsBusy;
  }
  OwnedSlot& slot = slots.slots[*slot_index];
  slot.stale = PixelRegion::Whole(static_cast<int>(source->image.width),
                                  static_cast<int>(source->image.height));
  if (!CopyIntoSlot(vulkan, *source, slots.extra.last_format, std::nullopt,
                    &slot, out)) {
    return DmabufCopy::kFailed;
  }
  out->slot_index = *slot_index;
  return DmabufCopy::kCopied;
}

void ReleaseAcceleratedDmabufFrame(uint64_t slot_token) {
  g_surfaces.Release(slot_token);
}

std::vector<uint64_t> RetireAcceleratedDmabufBrowser(int browser_id) {
  VulkanContext* vulkan = SharedVulkanContext();
  return g_surfaces.RetireBrowser(browser_id, [vulkan](SurfaceSlots& slots) {
    DestroySurface(vulkan, slots);
  });
}

}  // namespace sabine_osr
