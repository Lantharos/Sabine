// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// A handle value can name a new texture once CEF closed the old one, so the
// cache keeps its own handle to every texture to tell them apart.

#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_WINDOWS_SOURCE_CACHE_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_WINDOWS_SOURCE_CACHE_H_

#include <windows.h>

#include <d3d11.h>
#include <wrl/client.h>

#include <cstdint>
#include <map>
#include <memory>

namespace sabine_osr {

// CEF hands out the same few pooled textures frame after frame, so each one
// a browser surface paints into is opened once, and kept until the pool
// moves to another size or the cache fills and it was used longest ago.
class SourceCache {
 public:
  ID3D11Texture2D* Find(HANDLE cef_shared_handle);
  ID3D11Texture2D* Insert(HANDLE cef_shared_handle,
                          Microsoft::WRL::ComPtr<ID3D11Texture2D> texture);
  void ForgetOtherSizes(UINT width, UINT height);

 private:
  struct Source {
    Source() = default;
    Source(const Source&) = delete;
    Source& operator=(const Source&) = delete;
    ~Source();

    HANDLE handle = nullptr;
    Microsoft::WRL::ComPtr<ID3D11Texture2D> texture;
    UINT width = 0;
    UINT height = 0;
    uint64_t last_use = 0;
  };

  std::map<HANDLE, std::unique_ptr<Source>> sources_;
  uint64_t use_ = 0;
};

}  // namespace sabine_osr

#endif
