#ifndef SABINE_CEF_HOST_OSR_GUEST_GUESTS_H_
#define SABINE_CEF_HOST_OSR_GUEST_GUESTS_H_

#include <cstddef>
#include <string>
#include <vector>

#include "common/json.h"
#include "include/cef_browser.h"
#include "include/cef_render_handler.h"
#include "include/cef_request_context.h"
#include "osr/guest/registry.h"
#include "osr/guest/request.h"
#include "osr/transport/transport.h"

class SabineOsrHandler;

// The window's guest views: their browsers and the popups they open, which
// one has focus, and the request contexts of their partitions.
class Guests {
 public:
  Guests(SabineOsrHandler* owner, sabine_osr::Transport& transport)
      : owner_(owner), transport_(transport) {}
  Guests(const Guests&) = delete;
  Guests& operator=(const Guests&) = delete;

  GuestView* Find(const std::string& id) { return registry_.Find(id); }
  GuestView* FindByBrowser(const CefRefPtr<CefBrowser>& browser) {
    return registry_.FindByBrowser(browser);
  }
  // The guest `browser` shows, including the one whose browser is being
  // created, or null for the window's own page.
  GuestView* ForBrowser(const CefRefPtr<CefBrowser>& browser);
  GuestView* AwaitingBrowser();
  GuestView* TopmostAt(double x, double y) { return registry_.TopmostAt(x, y); }
  std::vector<GuestView*> InZOrder() { return registry_.InZOrder(); }
  bool Covered() const { return registry_.Covered(); }
  const std::string& focused_id() const { return focused_id_; }
  GuestView* Focused() { return registry_.Find(focused_id_); }

  bool HandleBridgeCommand(const std::string& command,
                           const std::string& payload,
                           const std::string& browser_id,
                           const std::string& request_id);
  void ApplyHostControl(const std::string& command, const std::string& value);
  void Create(GuestCreateRequest request, GuestCreateCallback callback);
  void ContinueCreate(const GuestCreateRequest& request,
                      CefRefPtr<CefRequestContext> context,
                      GuestCreateCallback callback);
  void RequestContextInitialized(const std::string& partition,
                                 CefRefPtr<CefRequestContext> context);
  // Opens what a guest's page asked to open in a new window, as the guest's
  // popup policy says.
  void OpenPopup(CefRefPtr<CefBrowser> browser,
                 const std::string& url,
                 int disposition,
                 bool user_gesture);
  void Destroy(const std::string& id);
  void DestroyAll();
  void Focus(const std::string& id);
  void DismissPopup();

  void ApplyBounds(GuestView& guest);
  void ApplyVisibility(GuestView& guest);
  void ApplyLifecycle();
  void NotifyScreenInfo();
  void FollowPageZoom(double zoom);
  void Repaint();

  // Notes the guest's first frame, which opens `sabine.popup`.
  void MarkPainted(GuestView& guest);
  bool SendPaint(const GuestView& guest,
                 const void* buffer,
                 int width,
                 int height,
                 const CefRenderHandler::RectList& dirty_rects);
  void SendHidden(const GuestView& guest);
  // Hides the guests once the page painted the cover it asked for.
  void HideCovered();
  // Where the open popup of `guest`'s page sits in the window's view.
  CefRect PopupInView(const GuestView& guest) const {
    return guest.ToView(popup_rect_);
  }
  void HidePopup(const GuestView& guest);
  void ResizePopup(const GuestView& guest, const CefRect& rect);
  void Emit(const GuestView& guest,
            const std::string& name,
            const std::string& payload);

 private:
  bool RunOperation(const std::string& operation,
                    const JsonObject& payload,
                    std::string* response,
                    std::string* error);
  bool CancelPending(const std::string& id);
  bool HasPending(const std::string& id) const;
  size_t PopupCount() const;
  // Tells the window the order guests stack in, bottom first, which it draws
  // them in.
  void SendOrder();
  void SendPopupHidden(const GuestView& guest);
  CefRefPtr<CefRequestContext> CreateRequestContext(
      const std::string& partition);
  std::string NextId();

  SabineOsrHandler* const owner_;
  sabine_osr::Transport& transport_;
  GuestRegistry registry_;
  GuestCreations creations_;
  std::string focused_id_;
  bool pending_cover_ = false;
  CefRect popup_rect_;
};

#endif
