#ifndef SABINE_CEF_HOST_OSR_ACCELERATED_PAINT_H_
#define SABINE_CEF_HOST_OSR_ACCELERATED_PAINT_H_

#include <cstdint>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <vector>

#include "include/cef_command_line.h"
#include "include/cef_render_handler.h"
#include "include/internal/cef_types.h"
#include "osr/accelerated/damage.h"
#include "osr/accelerated/drop.h"
#include "osr/accelerated/slot_ring.h"
#include "osr/paint/dropped.h"
#include "osr/transport/transport.h"
#if defined(OS_LINUX)
#include <chrono>

#include "osr/accelerated/linux/dmabuf_copy.h"
#endif
#if defined(OS_MAC)
#include <memory>

#include "osr/accelerated/macos/surface_broker.h"
#endif

class SabineOsrHandler;

namespace sabine_osr {

bool PreferSharedTexture(CefRefPtr<CefCommandLine> command_line);
void ApplySharedTexture(CefWindowInfo* window_info, bool enabled);

// Where a surface's accelerated frames go in the window.
struct AcceleratedTarget {
  uint32_t kind = 0;
  std::string guest_id;
  int32_t x = 0;
  int32_t y = 0;
};

// Accelerated paint: Chromium's shared textures, copied into frames the
// window composites.
class AcceleratedPaint {
 public:
  AcceleratedPaint(SabineOsrHandler* owner,
                   Transport& transport,
                   DroppedPaints& dropped)
      : owner_(owner), transport_(transport), dropped_(dropped) {}
  AcceleratedPaint(const AcceleratedPaint&) = delete;
  AcceleratedPaint& operator=(const AcceleratedPaint&) = delete;

  void Enable(bool enabled) { enabled_ = enabled; }
  bool enabled() const { return enabled_; }
  bool failed() const { return failed_; }
#if defined(OS_MAC)
  void UseSurfaceService(const std::string& service_name);
#endif

  // The part of `info`'s frame the page fills, or nothing when the window
  // can't show the frame.
  std::optional<CefRect> Accept(const CefAcceleratedPaintInfo& info);
  // What became of a frame: dropped before it was copied, or copied and then
  // sent or not.
  enum class Presented { kDropped, kUnsent, kSent };

  // Copies the frame and hands it to the window at `target`.
  Presented Present(CefRefPtr<CefBrowser> browser,
                    cef_paint_element_type_t type,
                    const CefRenderHandler::RectList& dirty_rects,
                    const CefAcceleratedPaintInfo& info,
                    const AcceleratedTarget& target,
                    const CefRect& visible);
  void Shown() { shown_ = true; }
  void Drop(AcceleratedDrop drop);
  // Makes the window take software frames, because no accelerated frame
  // will come for `reason`.
  void ReportUnavailable(AcceleratedDrop reason);
  // Reports when the loaded page still shows no accelerated frame after a
  // grace period.
  void Watch();

  void ReleaseSlot(uint64_t slot_token);
  void RetireBrowser(int browser_id);
  // Lets the window drop resources Sabine retired; resources this window was
  // never handed are ignored.
  void RetireResources(const std::vector<uint64_t>& resource_ids);

 private:
  struct CopiedFrame {
    uint32_t width = 0;
    uint32_t height = 0;
    uint64_t resource_id = 0;
    uint32_t resource_slot = 0;
    uint64_t shared_handle = 0;
    uint64_t slot_token = 0;
    PixelRegion damage;
    uint64_t sequence = 0;
  };

  // What a copied frame holds: its color format and the part the page fills.
  struct Content {
    uint32_t format = 0;
    CefRect visible;
  };

  bool Send(const AcceleratedTarget& target,
            const CopiedFrame& copied,
            const Content& content);
  bool Copy(const AcceleratedSurfaceKey& surface,
            const PixelRegion& damage,
            const CefAcceleratedPaintInfo& info,
            CopiedFrame* out);
  // Fills `out` for a frame `copied` into a slot, announcing the slot's
  // resource with `announce` the first time; `announce` gives the handle
  // the window opens it by, where it needs one.
  template <typename Announce>
  bool ShareCopiedSlot(const CopiedSlot& copied,
                       Announce announce,
                       CopiedFrame* out);
  void Discard(const CopiedFrame& frame);
  void RetireSurfaces(int browser_id);
  void SendRetire(const std::vector<uint64_t>& resource_ids);
  // Chromium reports damage against the capture before it, so a frame that
  // follows one Sabine never copied has to be copied whole.
  bool FollowsCopiedCapture(
      const AcceleratedSurfaceKey& surface,
      const cef_accelerated_paint_info_common_t& frame) const;
  void ReportMissing();
#if defined(OS_LINUX)
  void RepaintAllSoon();
  bool ShareDmabufCopy(DmabufCopy result,
                       const AccelDmabufCopiedFrame& copied,
                       CopiedFrame* out);

  // A page frame shown as copied while Chromium may still have been drawing
  // it, copied again once the page stops painting.
  struct UnsettledFrame {
    CefRefPtr<CefBrowser> browser;
    Content content;
    std::chrono::steady_clock::time_point painted_at;
  };

  void SettleWhenPaused(const AcceleratedSurfaceKey& surface,
                        CefRefPtr<CefBrowser> browser,
                        const Content& content);
  void ScheduleSettle(std::chrono::milliseconds delay);
  void SettlePausedFrames();
  void SettleFrame(const AcceleratedSurfaceKey& surface,
                   const UnsettledFrame& frame);
#endif

  SabineOsrHandler* const owner_;
  Transport& transport_;
  DroppedPaints& dropped_;
  std::vector<char> payload_;
  std::map<AcceleratedSurfaceKey, uint64_t> copied_captures_;
  std::set<uint64_t> announced_resources_;
#if defined(OS_MAC)
  std::unique_ptr<SurfaceBroker> surface_broker_;
#endif
  bool enabled_ = false;
  bool shown_ = false;
  bool failed_ = false;
#if defined(OS_LINUX)
  std::map<AcceleratedSurfaceKey, UnsettledFrame> unsettled_frames_;
  bool settle_pending_ = false;
  bool repaint_all_pending_ = false;
#endif
  AcceleratedDrop drop_;
};

}  // namespace sabine_osr

#endif
