#ifndef SABINE_CEF_HOST_OSR_PAINT_DROPPED_H_
#define SABINE_CEF_HOST_OSR_PAINT_DROPPED_H_

#include <list>
#include <set>

#include "include/cef_browser.h"
#include "osr/accelerated/damage.h"

namespace sabine_osr {

// Surfaces whose frame was dropped while every slot was in flight, painted
// again once one frees up.
class DroppedPaints {
 public:
  void Add(const AcceleratedSurfaceKey& surface) { surfaces_.insert(surface); }

  void Forget(int browser_id) {
    surfaces_.erase({browser_id, false});
    surfaces_.erase({browser_id, true});
  }

  void Repaint(const std::list<CefRefPtr<CefBrowser>>& browsers) {
    for (const AcceleratedSurfaceKey& surface : surfaces_) {
      for (const CefRefPtr<CefBrowser>& browser : browsers) {
        if (browser->GetIdentifier() == surface.browser_id) {
          browser->GetHost()->Invalidate(surface.popup ? PET_POPUP : PET_VIEW);
        }
      }
    }
    surfaces_.clear();
  }

 private:
  std::set<AcceleratedSurfaceKey> surfaces_;
};

}  // namespace sabine_osr

#endif
