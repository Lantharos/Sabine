#include "osr/accelerated/macos/iosurface_copy.h"

#import <Metal/Metal.h>

#include <CoreFoundation/CoreFoundation.h>
#include <CoreVideo/CVPixelBuffer.h>
#include <IOSurface/IOSurface.h>

#include <cstdio>
#include <map>

namespace sabine_osr {
namespace {

constexpr uint32_t kSlotsPerSurface = 4;

struct OwnedSurfaceSlot {
  OwnedSurfaceSlot() = default;
  OwnedSurfaceSlot(const OwnedSurfaceSlot&) = delete;
  OwnedSurfaceSlot& operator=(const OwnedSurfaceSlot&) = delete;
  ~OwnedSurfaceSlot() { Reset(); }

  IOSurfaceRef surface = nullptr;
  id<MTLTexture> texture = nil;
  uint64_t surface_id = 0;
  bool in_use = false;
  uint64_t token = 0;

  void Reset() {
    [texture release];
    texture = nil;
    if (surface) {
      CFRelease(surface);
      surface = nullptr;
    }
    surface_id = 0;
    in_use = false;
    token = 0;
  }
};

id<MTLDevice> g_device = nil;
id<MTLCommandQueue> g_queue = nil;
std::map<std::string, OwnedSurfaceSlot> g_slots;
std::map<std::string, uint32_t> g_next_slot;
uint64_t g_next_token = 1;
uint64_t g_next_surface_id = 1;

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

bool EnsureOwnedSlot(OwnedSurfaceSlot* slot,
                     IOSurfaceRef source,
                     MTLPixelFormat format,
                     uint64_t* replaced_surface_id) {
  const size_t width = IOSurfaceGetWidth(source);
  const size_t height = IOSurfaceGetHeight(source);
  const OSType pixel_format = IOSurfaceGetPixelFormat(source);
  if (slot->surface && IOSurfaceGetWidth(slot->surface) == width &&
      IOSurfaceGetHeight(slot->surface) == height &&
      IOSurfaceGetPixelFormat(slot->surface) == pixel_format) {
    return true;
  }
  *replaced_surface_id = slot->surface_id;
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
  slot->surface_id = g_next_surface_id++;
  return true;
}

bool CopyIntoSlot(id<MTLTexture> source,
                  IOSurfaceRef source_surface,
                  MTLPixelFormat format,
                  const std::string& slot_key,
                  AccelIOSurfaceCopiedFrame* out) {
  OwnedSurfaceSlot& slot = g_slots[slot_key];
  uint64_t replaced_surface_id = 0;
  if (slot.in_use ||
      !EnsureOwnedSlot(&slot, source_surface, format, &replaced_surface_id)) {
    return false;
  }
  id<MTLCommandBuffer> commands = [g_queue commandBuffer];
  id<MTLBlitCommandEncoder> blit = [commands blitCommandEncoder];
  [blit copyFromTexture:source toTexture:slot.texture];
  [blit endEncoding];
  [commands commit];
  [commands waitUntilCompleted];
  if (commands.status != MTLCommandBufferStatusCompleted) {
    std::fprintf(stderr, "Sabine CEF: shared paint copy failed\n");
    return false;
  }
  slot.in_use = true;
  slot.token = g_next_token++;
  out->surface = slot.surface;
  out->surface_id = slot.surface_id;
  out->replaced_surface_id = replaced_surface_id;
  out->slot_token = slot.token;
  out->width = static_cast<uint32_t>(IOSurfaceGetWidth(slot.surface));
  out->height = static_cast<uint32_t>(IOSurfaceGetHeight(slot.surface));
  return true;
}

}  // namespace

bool CopyAcceleratedIOSurfaceFrame(const std::string& slot_key,
                                   void* cef_io_surface,
                                   AccelIOSurfaceCopiedFrame* out) {
  IOSurfaceRef source_surface = static_cast<IOSurfaceRef>(cef_io_surface);
  if (!source_surface || !out || !EnsureDevice()) {
    return false;
  }
  const MTLPixelFormat format =
      MetalFormat(IOSurfaceGetPixelFormat(source_surface));
  if (format == MTLPixelFormatInvalid) {
    return false;
  }
  @autoreleasepool {
    id<MTLTexture> source = WrapSurface(source_surface, format);
    if (!source) {
      return false;
    }
    const uint32_t first_slot = g_next_slot[slot_key]++ % kSlotsPerSurface;
    bool copied = false;
    for (uint32_t offset = 0; offset < kSlotsPerSurface && !copied; ++offset) {
      const uint32_t slot_index = (first_slot + offset) % kSlotsPerSurface;
      copied = CopyIntoSlot(source, source_surface, format,
                            slot_key + "#" + std::to_string(slot_index), out);
    }
    [source release];
    return copied;
  }
}

void ReleaseAcceleratedIOSurfaceFrame(uint64_t slot_token) {
  for (auto& [key, slot] : g_slots) {
    (void)key;
    if (slot.in_use && slot.token == slot_token) {
      slot.in_use = false;
      slot.token = 0;
      return;
    }
  }
}

std::vector<uint64_t> RetireAcceleratedIOSurfaceBrowser(int browser_id) {
  const std::string prefix = std::to_string(browser_id) + "/";
  std::vector<uint64_t> retired;
  for (auto it = g_slots.begin(); it != g_slots.end();) {
    if (it->first.rfind(prefix, 0) == 0) {
      if (it->second.surface_id != 0) {
        retired.push_back(it->second.surface_id);
      }
      it = g_slots.erase(it);
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
