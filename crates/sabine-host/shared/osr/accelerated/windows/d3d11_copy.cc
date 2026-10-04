// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// CEF owns an accelerated-paint texture only until its callback returns. This
// module opens that texture on the same DXGI adapter, copies it into a Sabine-
// owned D3D12 shareable resource through D3D11, waits for an ordered D3D11
// fence, and retains the copied slot until the compositor acknowledges it.
// Seemingly redundant COM interfaces and handle transitions enforce those
// ownership and ordering rules. Some of it may look unnecessary.
// Unfortunately, Windows disagrees.
//
// If it works, assume there is a reason.

#include "osr/accelerated/windows/d3d11_copy.h"

#include <d3d11.h>
#include <d3d11_1.h>
#include <d3d11_4.h>
#include <d3d12.h>
#include <dxgi.h>
#include <dxgi1_4.h>
#include <wrl/client.h>

#include <array>
#include <cstdio>
#include <map>
#include <memory>

namespace sabine_osr {
namespace {

using Microsoft::WRL::ComPtr;

constexpr uint32_t kCefColorTypeBgra8888 = 1;
constexpr DWORD kGpuFenceTimeoutMs = 1000;
constexpr uint32_t kSlotsPerSurface = 4;

struct D3d11Context {
  ComPtr<ID3D11Device1> device;
  ComPtr<ID3D11DeviceContext4> context;
  ComPtr<ID3D11Fence> fence;
  HANDLE fence_event = nullptr;
  uint64_t fence_value = 0;
  ComPtr<ID3D12Device> device12;
};

struct OwnedSharedSlot {
  OwnedSharedSlot() = default;
  OwnedSharedSlot(const OwnedSharedSlot&) = delete;
  OwnedSharedSlot& operator=(const OwnedSharedSlot&) = delete;
  ~OwnedSharedSlot() { Reset(); }

  ComPtr<ID3D11Texture2D> texture;
  ComPtr<ID3D12Resource> resource12;
  HANDLE shared_handle = nullptr;
  int width = 0;
  int height = 0;
  DXGI_FORMAT format = DXGI_FORMAT_UNKNOWN;
  uint64_t resource_id = 0;
  bool in_use = false;
  uint64_t token = 0;
  PixelRegion stale;

  void Reset() {
    texture.Reset();
    resource12.Reset();
    if (shared_handle) {
      CloseHandle(shared_handle);
      shared_handle = nullptr;
    }
    width = 0;
    height = 0;
    format = DXGI_FORMAT_UNKNOWN;
    resource_id = 0;
    in_use = false;
    token = 0;
    stale = {};
  }
};

struct SurfaceSlots {
  std::array<OwnedSharedSlot, kSlotsPerSurface> slots;
  uint32_t next = 0;
};

std::unique_ptr<D3d11Context> g_d3d11;
std::map<AcceleratedSurfaceKey, SurfaceSlots> g_surfaces;
uint64_t g_next_token = 1;
uint64_t g_next_resource_id = 1;

void LogFailure(const char* what, HRESULT hr) {
  std::fprintf(stderr, "Sabine CEF: %s (hr=0x%08lx)\n", what,
               static_cast<unsigned long>(hr));
}

std::unique_ptr<D3d11Context> CreateContext(HANDLE shared_resource) {
  ComPtr<IDXGIFactory4> factory;
  HRESULT hr = CreateDXGIFactory1(IID_PPV_ARGS(&factory));
  LUID luid{};
  if (SUCCEEDED(hr)) {
    hr = factory->GetSharedResourceAdapterLuid(shared_resource, &luid);
  }
  ComPtr<IDXGIAdapter> adapter;
  if (SUCCEEDED(hr)) {
    hr = factory->EnumAdapterByLuid(luid, IID_PPV_ARGS(&adapter));
  }
  if (FAILED(hr)) {
    LogFailure("could not identify shared texture adapter", hr);
    return nullptr;
  }
  D3D_FEATURE_LEVEL feature_levels[] = {
      D3D_FEATURE_LEVEL_11_1,
      D3D_FEATURE_LEVEL_11_0,
  };
  ComPtr<ID3D11Device> device;
  ComPtr<ID3D11DeviceContext> context;
  hr = D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr, 0,
                         feature_levels, ARRAYSIZE(feature_levels),
                         D3D11_SDK_VERSION, &device, nullptr, &context);
  if (hr == E_INVALIDARG) {
    hr = D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr, 0,
                           &feature_levels[1], 1, D3D11_SDK_VERSION, &device,
                           nullptr, &context);
  }
  if (FAILED(hr)) {
    std::fprintf(stderr,
                 "Sabine CEF: D3D11 device creation failed on adapter %ld,%lu "
                 "(hr=0x%08lx)\n",
                 static_cast<long>(luid.HighPart),
                 static_cast<unsigned long>(luid.LowPart),
                 static_cast<unsigned long>(hr));
    return nullptr;
  }
  auto result = std::make_unique<D3d11Context>();
  ComPtr<ID3D11Device5> device5;
  if (FAILED(hr = device.As(&result->device)) ||
      FAILED(hr = device.As(&device5)) ||
      FAILED(hr = context.As(&result->context))) {
    LogFailure("D3D11 fence support is unavailable", hr);
    return nullptr;
  }
  ComPtr<IDXGIDevice> dxgi_device;
  ComPtr<IDXGIAdapter> device_adapter;
  if (FAILED(hr = device.As(&dxgi_device)) ||
      FAILED(hr = dxgi_device->GetAdapter(&device_adapter)) ||
      FAILED(hr =
                 D3D12CreateDevice(device_adapter.Get(), D3D_FEATURE_LEVEL_11_0,
                                   IID_PPV_ARGS(&result->device12)))) {
    LogFailure("could not create the D3D12 copy device", hr);
    return nullptr;
  }
  if (FAILED(hr = device5->CreateFence(0, D3D11_FENCE_FLAG_NONE,
                                       IID_PPV_ARGS(&result->fence)))) {
    LogFailure("could not create the D3D11 copy fence", hr);
    return nullptr;
  }
  result->fence_event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  if (!result->fence_event) {
    return nullptr;
  }
  return result;
}

bool WaitForGpu() {
  const uint64_t fence_value = ++g_d3d11->fence_value;
  if (FAILED(g_d3d11->context->Signal(g_d3d11->fence.Get(), fence_value)) ||
      FAILED(g_d3d11->fence->SetEventOnCompletion(fence_value,
                                                  g_d3d11->fence_event))) {
    return false;
  }
  g_d3d11->context->Flush();
  return WaitForSingleObject(g_d3d11->fence_event, kGpuFenceTimeoutMs) ==
         WAIT_OBJECT_0;
}

bool EnsureOwnedSharedSlot(OwnedSharedSlot* slot,
                           int width,
                           int height,
                           DXGI_FORMAT format,
                           std::vector<uint64_t>* retired) {
  if (slot->texture && slot->width == width && slot->height == height &&
      slot->format == format) {
    return true;
  }
  if (slot->resource_id != 0) {
    retired->push_back(slot->resource_id);
  }
  slot->Reset();

  D3D12_HEAP_PROPERTIES heap{};
  heap.Type = D3D12_HEAP_TYPE_DEFAULT;
  heap.CreationNodeMask = 1;
  heap.VisibleNodeMask = 1;
  D3D12_RESOURCE_DESC desc{};
  desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
  desc.Width = static_cast<UINT64>(width);
  desc.Height = static_cast<UINT>(height);
  desc.DepthOrArraySize = 1;
  desc.MipLevels = 1;
  desc.Format = format == DXGI_FORMAT_B8G8R8A8_UNORM
                    ? DXGI_FORMAT_B8G8R8A8_TYPELESS
                    : format;
  desc.SampleDesc.Count = 1;
  desc.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;
  desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET |
               D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS;

  HRESULT hr = g_d3d11->device12->CreateCommittedResource(
      &heap, D3D12_HEAP_FLAG_SHARED, &desc, D3D12_RESOURCE_STATE_COMMON,
      nullptr, IID_PPV_ARGS(&slot->resource12));
  if (FAILED(hr)) {
    LogFailure("failed to create owned D3D12 shared texture", hr);
    return false;
  }
  hr = g_d3d11->device12->CreateSharedHandle(slot->resource12.Get(), nullptr,
                                             GENERIC_ALL, nullptr,
                                             &slot->shared_handle);
  if (FAILED(hr)) {
    LogFailure("failed to export owned D3D11 shared handle", hr);
    slot->Reset();
    return false;
  }
  // Windows quirk: the D3D12 resource is the exported owner, while this D3D11
  // view exists solely so the copy can consume CEF's D3D11 texture.
  hr = g_d3d11->device->OpenSharedResource1(slot->shared_handle,
                                            IID_PPV_ARGS(&slot->texture));
  if (FAILED(hr)) {
    LogFailure("failed to open owned D3D12 texture in D3D11", hr);
    slot->Reset();
    return false;
  }
  slot->width = width;
  slot->height = height;
  slot->format = format;
  slot->resource_id = g_next_resource_id++;
  slot->stale = PixelRegion::Whole(width, height);
  return true;
}

bool CopyIntoSlot(ID3D11Texture2D* source,
                  const D3D11_TEXTURE2D_DESC& source_desc,
                  OwnedSharedSlot* slot,
                  AccelD3d11CopiedFrame* out) {
  const int width = static_cast<int>(source_desc.Width);
  const int height = static_cast<int>(source_desc.Height);
  if (!EnsureOwnedSharedSlot(slot, width, height, source_desc.Format,
                             &out->retired_resource_ids)) {
    return false;
  }
  const PixelRegion region = slot->stale.Within(width, height);
  if (!region.empty()) {
    const D3D11_BOX box{static_cast<UINT>(region.x),
                        static_cast<UINT>(region.y),
                        0,
                        static_cast<UINT>(region.x + region.width),
                        static_cast<UINT>(region.y + region.height),
                        1};
    g_d3d11->context->CopySubresourceRegion(slot->texture.Get(), 0, box.left,
                                            box.top, 0, source, 0, &box);
    if (!WaitForGpu()) {
      return false;
    }
  }
  slot->stale = {};
  slot->in_use = true;
  slot->token = g_next_token++;
  out->shared_handle = slot->shared_handle;
  out->resource_id = slot->resource_id;
  out->slot_token = slot->token;
  out->width = source_desc.Width;
  out->height = source_desc.Height;
  return true;
}

}  // namespace

bool CopyAcceleratedD3d11Frame(const AcceleratedSurfaceKey& surface,
                               HANDLE cef_shared_handle,
                               const PixelRegion& damage,
                               uint32_t cef_format,
                               AccelD3d11CopiedFrame* out) {
  if (!cef_shared_handle || cef_format != kCefColorTypeBgra8888) {
    return false;
  }
  if (!g_d3d11) {
    g_d3d11 = CreateContext(cef_shared_handle);
    if (!g_d3d11) {
      return false;
    }
  }
  // The returned COM reference must not outlive this paint callback. Sabine
  // exports its own copy instead of retaining CEF's pooled texture or handle.
  ComPtr<ID3D11Texture2D> source;
  const HRESULT hr = g_d3d11->device->OpenSharedResource1(
      cef_shared_handle, IID_PPV_ARGS(&source));
  if (FAILED(hr)) {
    LogFailure("OpenSharedResource1 failed", hr);
    return false;
  }
  D3D11_TEXTURE2D_DESC source_desc{};
  source->GetDesc(&source_desc);
  if (source_desc.Width == 0 || source_desc.Height == 0) {
    return false;
  }
  SurfaceSlots& slots = g_surfaces[surface];
  for (OwnedSharedSlot& slot : slots.slots) {
    slot.stale.Unite(damage);
  }
  const uint32_t first_slot = slots.next++ % kSlotsPerSurface;
  for (uint32_t offset = 0; offset < kSlotsPerSurface; ++offset) {
    const uint32_t slot_index = (first_slot + offset) % kSlotsPerSurface;
    OwnedSharedSlot& slot = slots.slots[slot_index];
    if (slot.in_use) {
      continue;
    }
    if (!CopyIntoSlot(source.Get(), source_desc, &slot, out)) {
      return false;
    }
    out->slot_index = slot_index;
    return true;
  }
  return false;
}

std::vector<uint64_t> RetireAcceleratedD3d11Browser(int browser_id) {
  std::vector<uint64_t> retired;
  for (auto it = g_surfaces.begin(); it != g_surfaces.end();) {
    if (it->first.browser_id != browser_id) {
      ++it;
      continue;
    }
    for (const OwnedSharedSlot& slot : it->second.slots) {
      if (slot.resource_id != 0) {
        retired.push_back(slot.resource_id);
      }
    }
    it = g_surfaces.erase(it);
  }
  return retired;
}

void ReleaseAcceleratedD3d11Frame(uint64_t slot_token) {
  for (auto& [surface, slots] : g_surfaces) {
    for (OwnedSharedSlot& slot : slots.slots) {
      if (slot.in_use && slot.token == slot_token) {
        slot.in_use = false;
        slot.token = 0;
        return;
      }
    }
  }
}

}  // namespace sabine_osr
