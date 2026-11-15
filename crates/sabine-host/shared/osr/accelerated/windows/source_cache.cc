// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// A handle value can name a new texture once CEF closed the old one, so the
// cache keeps its own handle to every texture to tell them apart.

#include "osr/accelerated/windows/source_cache.h"

#include <algorithm>

namespace sabine_osr {
namespace {

constexpr size_t kMaxCachedSources = 8;

// CompareObjectHandles lives in kernelbase.dll, whose import library the
// Windows SDK doesn't put on the default linker path.
bool SameObject(HANDLE first, HANDLE second) {
  using Compare = BOOL(WINAPI*)(HANDLE, HANDLE);
  static const auto compare = reinterpret_cast<Compare>(GetProcAddress(
      GetModuleHandleW(L"kernelbase.dll"), "CompareObjectHandles"));
  return compare(first, second);
}

}  // namespace

SourceCache::Source::~Source() {
  if (handle) {
    CloseHandle(handle);
  }
}

ID3D11Texture2D* SourceCache::Find(HANDLE cef_shared_handle) {
  const auto cached = sources_.find(cef_shared_handle);
  if (cached == sources_.end()) {
    return nullptr;
  }
  if (SameObject(cached->second->handle, cef_shared_handle)) {
    cached->second->last_use = ++use_;
    return cached->second->texture.Get();
  }
  sources_.erase(cached);
  return nullptr;
}

ID3D11Texture2D* SourceCache::Insert(
    HANDLE cef_shared_handle,
    Microsoft::WRL::ComPtr<ID3D11Texture2D> texture) {
  auto source = std::make_unique<Source>();
  if (!DuplicateHandle(GetCurrentProcess(), cef_shared_handle,
                       GetCurrentProcess(), &source->handle, 0, FALSE,
                       DUPLICATE_SAME_ACCESS)) {
    return nullptr;
  }
  if (sources_.size() >= kMaxCachedSources) {
    sources_.erase(std::min_element(
        sources_.begin(), sources_.end(), [](const auto& a, const auto& b) {
          return a.second->last_use < b.second->last_use;
        }));
  }
  D3D11_TEXTURE2D_DESC desc{};
  texture->GetDesc(&desc);
  source->texture = std::move(texture);
  source->width = desc.Width;
  source->height = desc.Height;
  source->last_use = ++use_;
  ID3D11Texture2D* opened = source->texture.Get();
  sources_[cef_shared_handle] = std::move(source);
  return opened;
}

void SourceCache::ForgetOtherSizes(UINT width, UINT height) {
  std::erase_if(sources_, [&](const auto& entry) {
    return entry.second->width != width || entry.second->height != height;
  });
}

}  // namespace sabine_osr
