#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_DAMAGE_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_DAMAGE_H_

#include <algorithm>
#include <tuple>

namespace sabine_osr {

// The browser surface a run of accelerated frames belongs to.
struct AcceleratedSurfaceKey {
  int browser_id = 0;
  bool popup = false;

  bool operator<(const AcceleratedSurfaceKey& other) const {
    return std::tie(browser_id, popup) <
           std::tie(other.browser_id, other.popup);
  }
};

// A rectangle of texture pixels that changed since a slot last received a
// copy, grown to cover every frame the slot missed.
struct PixelRegion {
  int x = 0;
  int y = 0;
  int width = 0;
  int height = 0;

  static PixelRegion Whole(int width, int height) {
    return PixelRegion{0, 0, width, height};
  }

  bool empty() const { return width <= 0 || height <= 0; }

  void Unite(const PixelRegion& other) {
    if (other.empty()) {
      return;
    }
    if (empty()) {
      *this = other;
      return;
    }
    const int right = std::max(x + width, other.x + other.width);
    const int bottom = std::max(y + height, other.y + other.height);
    x = std::min(x, other.x);
    y = std::min(y, other.y);
    width = right - x;
    height = bottom - y;
  }

  PixelRegion Within(int surface_width, int surface_height) const {
    const int left = std::clamp(x, 0, surface_width);
    const int top = std::clamp(y, 0, surface_height);
    const int right = std::clamp(x + width, left, surface_width);
    const int bottom = std::clamp(y + height, top, surface_height);
    return PixelRegion{left, top, right - left, bottom - top};
  }
};

}  // namespace sabine_osr

#endif
