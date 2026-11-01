#include "osr/accelerated/macos/iosurface_copy.h"

#import <Metal/Metal.h>

#include <CoreFoundation/CoreFoundation.h>
#include <CoreVideo/CVPixelBuffer.h>
#include <IOSurface/IOSurface.h>

#include <cstdio>
#include <map>

#include "osr/accelerated/slot_ring.h"

namespace sabine_osr {
namespace {

constexpr size_t kMaxCachedSources = 8;

struct OwnedSurfaceSlot : RingSlot {
  OwnedSurfaceSlot() = default;
  OwnedSurfaceSlot(const OwnedSurfaceSlot&) = delete;
  OwnedSurfaceSlot& operator=(const OwnedSurfaceSlot&) = delete;
  ~OwnedSurfaceSlot() { Reset(); }

  IOSurfaceRef surface = nullptr;
  id<MTLTexture> texture = nil;

  void Reset() {
    [texture release];
    texture = nil;
    if (surface) {
      CFRelease(surface);
      surface = nullptr;
    }
    resource_id = 0;
    in_use = false;
    token = 0;
    stale = {};
  }
};

// CEF hands out the same few pooled surfaces frame after frame, so each one
// is wrapped as a Metal texture once and kept while it stays in the pool.
struct CachedSource {
  IOSurfaceRef surface = nullptr;
  id<MTLTexture> texture = nil;
};

using SurfaceSlots = SurfaceRing<OwnedSurfaceSlot, NoExtra>;

id<MTLDevice> g_device = nil;
id<MTLCommandQueue> g_queue = nil;
SlotRings<OwnedSurfaceSlot> g_surfaces;
std::map<IOSurfaceRef, CachedSource> g_sources;

bool EnsureDevice() {
  if (g_queue) {
    return true;
  }
  g_device = MTLCreateSystemDefaultDevice();
  if (!g_device) {
    std::fprintf(stderr, "Sabine CEF: Metal is unavailable for shared paint\n");
    return false;
  }
  g_queue = [g_device newCommandQueue];
  return g_queue != nil;
}

MTLPixelFormat MetalFormat(OSType pixel_format) {
  switch (pixel_format) {
    case kCVPixelFormatType_32BGRA:
      return MTLPixelFormatBGRA8Unorm;
    case kCVPixelFormatType_32RGBA:
      return MTLPixelFormatRGBA8Unorm;
    default:
      return MTLPixelFormatInvalid;
  }
}

void SetNumber(CFMutableDictionaryRef properties, CFStringRef key, long value) {
  CFNumberRef number = CFNumberCreate(nullptr, kCFNumberLongType, &value);
  CFDictionarySetValue(properties, key, number);
  CFRelease(number);
}

IOSurfaceRef CreateSurface(size_t width, size_t height, OSType pixel_format) {
  CFMutableDictionaryRef properties =
      CFDictionaryCreateMutable(nullptr, 0, &kCFTypeDictionaryKeyCallBacks,
                                &kCFTypeDictionaryValueCallBacks);
  SetNumber(properties, kIOSurfaceWidth, static_cast<long>(width));
  SetNumber(properties, kIOSurfaceHeight, static_cast<long>(height));
  SetNumber(properties, kIOSurfaceBytesPerElement, 4);
  SetNumber(properties, kIOSurfacePixelFormat, static_cast<long>(pixel_format));
  SetNumber(properties, kIOSurfaceBytesPerRow,
            static_cast<long>(
                IOSurfaceAlignProperty(kIOSurfaceBytesPerRow, width * 4)));
  IOSurfaceRef surface = IOSurfaceCreate(properties);
  CFRelease(properties);
  return surface;
}

id<MTLTexture> WrapSurface(IOSurfaceRef surface, MTLPixelFormat format) {
  MTLTextureDescriptor* descriptor = [MTLTextureDescriptor
      texture2DDescriptorWithPixelFormat:format
                                   width:IOSurfaceGetWidth(surface)
                                  height:IOSurfaceGetHeight(surface)
                               mipmapped:NO];
  descriptor.usage = MTLTextureUsageShaderRead;
  descriptor.storageMode = MTLStorageModeShared;
  return [g_device newTextureWithDescriptor:descriptor
                                  iosurface:surface
                                      plane:0];
}

void ReleaseSources() {
  for (auto& [surface, source] : g_sources) {
    [source.texture release];
    CFRelease(source.surface);
  }
  g_sources.clear();
}

id<MTLTexture> SourceTexture(IOSurfaceRef surface, MTLPixelFormat format) {
  const auto cached = g_sources.find(surface);
  if (cached != g_sources.end() &&
      cached->second.texture.pixelFormat == format &&
      cached->second.texture.width == IOSurfaceGetWidth(surface) &&
      cached->second.texture.height == IOSurfaceGetHeight(surface)) {
    return cached->second.texture;
  }
  const bool resized =
      !g_sources.empty() &&
      g_sources.begin()->second.texture.width != IOSurfaceGetWidth(surface);
  if (cached != g_sources.end() || resized ||
      g_sources.size() >= kMaxCachedSources) {
    ReleaseSources();
  }
  id<MTLTexture> texture = WrapSurface(surface, format);
  if (!texture) {
    return nil;
  }
  CFRetain(surface);
  g_sources[surface] = CachedSource{surface, texture};
  return texture;
}

bool EnsureOwnedSlot(OwnedSurfaceSlot* slot,
                     IOSurfaceRef source,
                     MTLPixelFormat format,
                     std::vector<uint64_t>* retired) {
  const size_t width = IOSurfaceGetWidth(source);
  const size_t height = IOSurfaceGetHeight(source);
  const OSType pixel_format = IOSurfaceGetPixelFormat(source);
  if (slot->surface && IOSurfaceGetWidth(slot->surface) == width &&
      IOSurfaceGetHeight(slot->surface) == height &&
      IOSurfaceGetPixelFormat(slot->surface) == pixel_format) {
    return true;
  }
  if (slot->resource_id != 0) {
    retired->push_back(slot->resource_id);
  }
  slot->Reset();
  IOSurfaceRef surface = CreateSurface(width, height, pixel_format);
  if (!surface) {
    std::fprintf(stderr, "Sabine CEF: could not create a shared IOSurface\n");
    return false;
  }
  id<MTLTexture> texture = WrapSurface(surface, format);
  if (!texture) {
    CFRelease(surface);
    return false;
  }
  slot->surface = surface;
  slot->texture = texture;
  slot->resource_id = g_surfaces.NewResourceId();
  slot->stale =
      PixelRegion::Whole(static_cast<int>(width), static_cast<int>(height));
  return true;
}

// CEF's surface goes back to Chromium's pool when the paint callback
// returns, and Chromium neither checks whether it is still in use nor waits
// for another process's Metal work on it, so the copy has finished reading
// before the callback returns.
bool Copy(id<MTLTexture> source,
          id<MTLTexture> target,
          const PixelRegion& region) {
  id<MTLCommandBuffer> commands = [g_queue commandBuffer];
  id<MTLBlitCommandEncoder> blit = [commands blitCommandEncoder];
  const MTLOrigin origin = MTLOriginMake(region.x, region.y, 0);
  [blit copyFromTexture:source
            sourceSlice:0
            sourceLevel:0
           sourceOrigin:origin
             sourceSize:MTLSizeMake(region.width, region.height, 1)
              toTexture:target
       destinationSlice:0
       destinationLevel:0
      destinationOrigin:origin];
  [blit endEncoding];
  [commands commit];
  [commands waitUntilCompleted];
  if (commands.status != MTLCommandBufferStatusCompleted) {
    std::fprintf(stderr, "Sabine CEF: shared paint copy failed\n");
    return false;
  }
  return true;
}

bool CopyIntoSlot(id<MTLTexture> source,
                  IOSurfaceRef source_surface,
                  MTLPixelFormat format,
                  OwnedSurfaceSlot* slot,
                  AccelIOSurfaceCopiedFrame* out) {
  if (!EnsureOwnedSlot(slot, source_surface, format,
                       &out->retired_resource_ids)) {
    return false;
  }
  const int width = static_cast<int>(IOSurfaceGetWidth(slot->surface));
  const int height = static_cast<int>(IOSurfaceGetHeight(slot->surface));
  const PixelRegion region = slot->stale.Within(width, height);
  if (!region.empty() && !Copy(source, slot->texture, region)) {
    return false;
  }
  out->slot_token = g_surfaces.Claim(*slot);
  out->surface = slot->surface;
  out->resource_id = slot->resource_id;
  out->width = static_cast<uint32_t>(width);
  out->height = static_cast<uint32_t>(height);
  return true;
}

}  // namespace

bool CopyAcceleratedIOSurfaceFrame(const AcceleratedSurfaceKey& surface,
                                   void* cef_io_surface,
                                   const PixelRegion& damage,
                                   AccelIOSurfaceCopiedFrame* out) {
  IOSurfaceRef source_surface = static_cast<IOSurfaceRef>(cef_io_surface);
  if (!source_surface || !EnsureDevice()) {
    return false;
  }
  const MTLPixelFormat format =
      MetalFormat(IOSurfaceGetPixelFormat(source_surface));
  if (format == MTLPixelFormatInvalid) {
    return false;
  }
  @autoreleasepool {
    id<MTLTexture> source = SourceTexture(source_surface, format);
    if (!source) {
      return false;
    }
    SurfaceSlots& slots = g_surfaces[surface];
    slots.Damage(damage);
    const std::optional<uint32_t> slot_index = slots.FreeSlot();
    if (!slot_index || !CopyIntoSlot(source, source_surface, format,
                                     &slots.slots[*slot_index], out)) {
      return false;
    }
    out->slot_index = *slot_index;
    return true;
  }
}

void ReleaseAcceleratedIOSurfaceFrame(uint64_t slot_token) {
  g_surfaces.Release(slot_token);
}

std::vector<uint64_t> RetireAcceleratedIOSurfaceBrowser(int browser_id) {
  std::vector<uint64_t> retired =
      g_surfaces.RetireBrowser(browser_id, [](SurfaceSlots&) {});
  if (g_surfaces.empty()) {
    ReleaseSources();
  }
  return retired;
}

}  // namespace sabine_osr
