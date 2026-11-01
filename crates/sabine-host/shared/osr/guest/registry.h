#ifndef SABINE_CEF_HOST_OSR_GUEST_REGISTRY_H_
#define SABINE_CEF_HOST_OSR_GUEST_REGISTRY_H_

#include <map>
#include <string>
#include <vector>

#include "include/cef_browser.h"
#include "include/cef_values.h"
#include "osr/guest/request.h"

// Guest id reserved for the `sabine.popup` surface.
extern const char kSabinePopupGuestId[];
// Guest id of the DevTools docked in development windows, which pages neither
// see nor manage.
extern const char kSabineDevToolsGuestId[];

// Whether |id| names one of Sabine's own surfaces, which pages neither see
// nor manage through `sabine.guest`.
bool IsInternalGuestId(const std::string& id);

struct GuestView {
  std::string id;
  CefRefPtr<CefBrowser> browser;
  CefRect bounds;
  std::string url;
  std::string title;
  std::string partition;
  bool visible = true;
  bool allow_bridge = false;
  CefRefPtr<CefDictionaryValue> bridge_policy;
  bool allow_downloads = true;
  std::vector<std::string> intercepted_shortcuts;
  bool intercept_horizontal_wheel = false;
  bool pending = false;
  bool painted = false;
  bool loading = false;
  GuestPopupPolicy popup_policy = GuestPopupPolicy::kDeny;
  double zoom = 1.0;

  bool ShownToPage() const { return !IsInternalGuestId(id); }
};

/// Guest views keyed by id, with a separate bottom-to-top z-order used for
/// pointer hit testing and paint ordering.
class GuestRegistry {
 public:
  GuestView* Find(const std::string& id);
  const GuestView* Find(const std::string& id) const;
  GuestView* FindByBrowser(const CefRefPtr<CefBrowser>& browser);
  GuestView* Insert(GuestView guest);
  void Erase(const std::string& id);
  void Raise(const std::string& id);
  GuestView* TopmostAt(int x, int y);
  std::vector<GuestView*> InZOrder();
  std::vector<const GuestView*> InZOrder() const;
  void SetCovered(bool covered) { covered_ = covered; }
  bool Covered() const { return covered_; }

 private:
  std::map<std::string, GuestView> guests_;
  std::vector<std::string> z_order_;
  bool covered_ = false;
};

#endif
