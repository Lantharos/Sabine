// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// CEF owns an accelerated-paint texture only until its callback returns. This
// module opens that texture on the same DXGI adapter, copies it into a Sabine-
// owned D3D12 shareable resource through D3D11, waits for an ordered D3D11
// fence, and retains the copied slot until the compositor acknowledges it.
// Seemingly redundant COM interfaces and handle transitions enforce those
// ownership and ordering rules. Some of it may look unnecessary. Unfortunately,
// Windows disagrees.
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

#include <chrono>
#include <cstdio>
#include <map>
#include <memory>

#include "osr/accelerated/slot_ring.h"

namespace sabine_osr {
namespace {

using Microsoft::WRL::ComPtr;

constexpr std::chrono::milliseconds kGpuFenceTimeout{1000};
constexpr size_t kMaxCachedSources = 8;

struct D3d11Context {
  ~D3d11Context() {
    if (fence_event) {
      CloseHandle(fence_event);
    }
  }

  LUID luid{};
  ComPtr<ID3D11Device1> device;
  ComPtr<ID3D11DeviceContext4> context;
  ComPtr<ID3D11Fence> fence;
  HANDLE fence_event = nullptr;
  uint64_t fence_value = 0;
  ComPtr<ID3D12Device> device12;
};

struct OwnedSharedSlot : RingSlot {
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

// CEF hands out the same few pooled textures frame after frame, so each is
// opened once. A handle value can name a new texture once CEF closed the old
// one, so the cache keeps its own handle to tell them apart.
struct CachedSource {
  CachedSource() = default;
  CachedSource(const CachedSource&) = delete;
  CachedSource& operator=(const CachedSource&) = delete;
  ~CachedSource() {
    if (handle) {
      CloseHandle(handle);
    }
  }

  HANDLE handle = nullptr;
  ComPtr<ID3D11Texture2D> texture;
};

using SurfaceSlots = SurfaceRing<OwnedSharedSlot, NoExtra>;

std::unique_ptr<D3d11Context> g_d3d11;
SlotRings<OwnedSharedSlot> g_surfaces;
std::map<HANDLE, std::unique_ptr<CachedSource>> g_sources;

void LogFailure(const char* what, HRESULT hr) {
  std::fprintf(stderr, "Sabine CEF: %s (hr=0x%08lx)\n", what,
               static_cast<unsigned long>(hr));
}

bool SharedResourceAdapter(HANDLE shared_resource,
                           ComPtr<IDXGIFactory4>* factory,
                           LUID* luid) {
  HRESULT hr =
      CreateDXGIFactory1(IID_PPV_ARGS(factory->ReleaseAndGetAddressOf()));
  if (SUCCEEDED(hr)) {
    hr = (*factory)->GetSharedResourceAdapterLuid(shared_resource, luid);
  }
  if (FAILED(hr)) {
    LogFailure("could not identify shared texture adapter", hr);
    return false;
  }
  return true;
}

bool CreateCopyFence(ID3D11Device5* device, D3d11Context* context) {
  const HRESULT hr = device->CreateFence(0, D3D11_FENCE_FLAG_NONE,
                                         IID_PPV_ARGS(&context->fence));
  if (FAILED(hr)) {
    LogFailure("could not create the D3D11 copy fence", hr);
    return false;
  }
  context->fence_event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  return context->fence_event != nullptr;
}

std::unique_ptr<D3d11Context> CreateContext(HANDLE shared_resource) {
  ComPtr<IDXGIFactory4> factory;
  LUID luid{};
  if (!SharedResourceAdapter(shared_resource, &factory, &luid)) {
    return nullptr;
  }
  ComPtr<IDXGIAdapter> adapter;
  HRESULT hr = factory->EnumAdapterByLuid(luid, IID_PPV_ARGS(&adapter));
  if (FAILED(hr)) {
    LogFailure("could not open shared texture adapter", hr);
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
  result->luid = luid;
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
  if (!CreateCopyFence(device5.Get(), result.get())) {
    return nullptr;
  }
  return result;
}

// Waits until the fence reaches `value`. The event is auto-reset and may
// still carry the signal of an earlier wait that timed out, so the fence's
// completed value decides, not the event.
bool WaitForFence(uint64_t value) {
  const auto deadline = std::chrono::steady_clock::now() + kGpuFenceTimeout;
  while (g_d3d11->fence->GetCompletedValue() < value) {
    const auto remaining = std::chrono::ceil<std::chrono::milliseconds>(
                               deadline - std::chrono::steady_clock::now())
                               .count();
    if (remaining <= 0 || FAILED(g_d3d11->fence->SetEventOnCompletion(
                              value, g_d3d11->fence_event))) {
      return false;
    }
    WaitForSingleObject(g_d3d11->fence_event, static_cast<DWORD>(remaining));
  }
  return true;
}

DXGI_FORMAT TypelessFormat(DXGI_FORMAT format) {
  return format == DXGI_FORMAT_B8G8R8A8_UNORM ? DXGI_FORMAT_B8G8R8A8_TYPELESS
                                              : DXGI_FORMAT_R8G8B8A8_TYPELESS;
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
  desc.Format = TypelessFormat(format);
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
  slot->resource_id = g_surfaces.NewResourceId();
  slot->stale = PixelRegion::Whole(width, height);
  return true;
}

// CEF's texture goes back to Chromium's pool when the paint callback
// returns, and Chromium's next draw into it waits for nothing of ours: CEF
// removes the keyed mutex and Chromium fences only its own devices. So the
// copy has finished reading before the callback returns. `lost` reports a
// copy that never finished.
bool CopyIntoSlot(ID3D11Texture2D* source,
                  const D3D11_TEXTURE2D_DESC& source_desc,
                  OwnedSharedSlot* slot,
                  AccelD3d11CopiedFrame* out,
                  bool* lost) {
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
    const uint64_t value = ++g_d3d11->fence_value;
    if (FAILED(g_d3d11->context->Signal(g_d3d11->fence.Get(), value))) {
      return false;
    }
    g_d3d11->context->Flush();
    if (!WaitForFence(value)) {
      *lost = true;
      return false;
    }
  }
  out->slot_token = g_surfaces.Claim(*slot);
  out->shared_handle = slot->shared_handle;
  out->resource_id = slot->resource_id;
  out->width = source_desc.Width;
  out->height = source_desc.Height;
  return true;
}

// The copy device stops working when its adapter is reset or removed, and
// Chromium moves to another adapter when the window does. Either way, every
// slot made on the old device goes with it.
bool ContextIsStale(HANDLE cef_shared_handle) {
  if (FAILED(g_d3d11->device->GetDeviceRemovedReason())) {
    return true;
  }
  ComPtr<IDXGIFactory4> factory;
  LUID luid{};
  return SharedResourceAdapter(cef_shared_handle, &factory, &luid) &&
         (luid.LowPart != g_d3d11->luid.LowPart ||
          luid.HighPart != g_d3d11->luid.HighPart);
}

void ResetContext(std::vector<uint64_t>* retired) {
  const std::vector<uint64_t> slots =
      g_surfaces.RetireAll([](SurfaceSlots&) {});
  retired->insert(retired->end(), slots.begin(), slots.end());
  g_sources.clear();
  g_d3d11.reset();
}

// CompareObjectHandles lives in kernelbase.dll, whose import library the
// Windows SDK doesn't put on the default linker path.
bool SameObject(HANDLE first, HANDLE second) {
  using Compare = BOOL(WINAPI*)(HANDLE, HANDLE);
  static const auto compare = reinterpret_cast<Compare>(GetProcAddress(
      GetModuleHandleW(L"kernelbase.dll"), "CompareObjectHandles"));
  return compare(first, second);
}

ID3D11Texture2D* CachedTexture(HANDLE cef_shared_handle) {
  const auto cached = g_sources.find(cef_shared_handle);
  if (cached == g_sources.end()) {
    return nullptr;
  }
  if (SameObject(cached->second->handle, cef_shared_handle)) {
    return cached->second->texture.Get();
  }
  g_sources.erase(cached);
  return nullptr;
}

ID3D11Texture2D* Cache(HANDLE cef_shared_handle,
                       ComPtr<ID3D11Texture2D> texture) {
  auto source = std::make_unique<CachedSource>();
  if (!DuplicateHandle(GetCurrentProcess(), cef_shared_handle,
                       GetCurrentProcess(), &source->handle, 0, FALSE,
                       DUPLICATE_SAME_ACCESS)) {
    return nullptr;
  }
  if (g_sources.size() >= kMaxCachedSources) {
    g_sources.clear();
  }
  source->texture = std::move(texture);
  ID3D11Texture2D* opened = source->texture.Get();
  g_sources[cef_shared_handle] = std::move(source);
  return opened;
}

// The returned texture stays cached while CEF keeps it in its pool, but its
// pixels are only Chromium's frame until this paint callback returns.
ID3D11Texture2D* OpenCefTexture(HANDLE cef_shared_handle,
                                std::vector<uint64_t>* retired) {
  if (g_d3d11) {
    if (ID3D11Texture2D* cached = CachedTexture(cef_shared_handle)) {
      return cached;
    }
    ComPtr<ID3D11Texture2D> texture;
    const HRESULT hr = g_d3d11->device->OpenSharedResource1(
        cef_shared_handle, IID_PPV_ARGS(&texture));
    if (SUCCEEDED(hr)) {
      return Cache(cef_shared_handle, std::move(texture));
    }
    if (!ContextIsStale(cef_shared_handle)) {
      LogFailure("OpenSharedResource1 failed", hr);
      return nullptr;
    }
    std::fprintf(stderr,
                 "Sabine CEF: the graphics device was reset; recreating the "
                 "copy device\n");
    ResetContext(retired);
  }
  g_d3d11 = CreateContext(cef_shared_handle);
  if (!g_d3d11) {
    return nullptr;
  }
  ComPtr<ID3D11Texture2D> texture;
  const HRESULT hr = g_d3d11->device->OpenSharedResource1(
      cef_shared_handle, IID_PPV_ARGS(&texture));
  if (FAILED(hr)) {
    LogFailure("OpenSharedResource1 failed", hr);
    return nullptr;
  }
  return Cache(cef_shared_handle, std::move(texture));
}

bool SupportedFormat(DXGI_FORMAT format) {
  return format == DXGI_FORMAT_B8G8R8A8_UNORM ||
         format == DXGI_FORMAT_R8G8B8A8_UNORM;
}

}  // namespace

bool CopyAcceleratedD3d11Frame(const AcceleratedSurfaceKey& surface,
                               HANDLE cef_shared_handle,
                               const PixelRegion& damage,
                               AccelD3d11CopiedFrame* out) {
  if (!cef_shared_handle) {
    return false;
  }
  ID3D11Texture2D* source =
      OpenCefTexture(cef_shared_handle, &out->retired_resource_ids);
  if (!source) {
    return false;
  }
  D3D11_TEXTURE2D_DESC source_desc{};
  source->GetDesc(&source_desc);
  if (source_desc.Width == 0 || source_desc.Height == 0 ||
      !SupportedFormat(source_desc.Format)) {
    return false;
  }
  SurfaceSlots& slots = g_surfaces[surface];
  slots.Damage(damage);
  const std::optional<uint32_t> slot_index = slots.FreeSlot();
  if (!slot_index) {
    return false;
  }
  bool lost = false;
  if (!CopyIntoSlot(source, source_desc, &slots.slots[*slot_index], out,
                    &lost)) {
    if (lost) {
      std::fprintf(stderr,
                   "Sabine CEF: a frame copy never finished; recreating the "
                   "copy device\n");
      ResetContext(&out->retired_resource_ids);
    }
    return false;
  }
  out->slot_index = *slot_index;
  return true;
}

std::vector<uint64_t> RetireAcceleratedD3d11Browser(int browser_id) {
  std::vector<uint64_t> retired =
      g_surfaces.RetireBrowser(browser_id, [](SurfaceSlots&) {});
  if (g_surfaces.empty()) {
    g_sources.clear();
  }
  return retired;
}

void ReleaseAcceleratedD3d11Frame(uint64_t slot_token) {
  g_surfaces.Release(slot_token);
}

}  // namespace sabine_osr
