#ifndef SABINE_CEF_HOST_OSR_GUEST_REQUEST_H_
#define SABINE_CEF_HOST_OSR_GUEST_REQUEST_H_

#include <functional>
#include <map>
#include <set>
#include <string>
#include <vector>

#include "common/json.h"
#include "include/cef_request_context.h"

extern const char kGuestBridgePrefix[];
extern const char kGuestHostControlPrefix[];

enum class GuestPopupPolicy {
  kDeny = 0,
  kAllow = 1,
  kNavigateSame = 2,
  kOpenGuest = 3,
};

GuestPopupPolicy ParseGuestPopupPolicy(const std::string& value,
                                       GuestPopupPolicy fallback);
const char* GuestPopupPolicyName(GuestPopupPolicy policy);
const char* WindowOpenDispositionName(int disposition);

struct GuestCreateRequest {
  std::string html;
  std::string id;
  std::string url;
  CefRect bounds = CefRect(0, 0, 1, 1);
  std::string partition;
  bool allow_bridge = false;
  bool allow_downloads = true;
  std::vector<std::string> intercepted_shortcuts;
  bool intercept_horizontal_wheel = false;
  bool visible = true;
  GuestPopupPolicy popup_policy = GuestPopupPolicy::kDeny;
  std::string background_color;
};

using GuestCreateCallback =
    std::function<void(bool success, const std::string& result)>;

// Guests waiting for their partition's request context, and the contexts
// partitions already have.
struct GuestCreations {
  struct Pending {
    GuestCreateRequest request;
    GuestCreateCallback callback;
  };

  std::string creating_id;
  std::map<std::string, CefRefPtr<CefRequestContext>> contexts;
  std::set<std::string> initialized_contexts;
  std::map<std::string, std::vector<Pending>> pending;
  int serial = 0;
};

/// Resolves the `url` / `html` pair of a create payload into a loadable URL and
/// validates the optional guest id. Returns false and fills |error| when the
/// payload has neither document nor a usable id.
bool ParseGuestCreateRequest(const JsonObject& payload,
                             GuestCreateRequest* request,
                             std::string* error);
CefRect ParseGuestBounds(const JsonObject& payload, const CefRect& fallback);

bool IsValidGuestId(const std::string& id);
bool IsGuestBridgeCommand(const std::string& command);
std::string GuestOperationName(const std::string& command,
                               const std::string& prefix);
std::string DefaultGuestPartition(const std::string& id);
// The profile folder a guest partition keeps its data in.
std::string GuestFolder(const std::string& partition);
std::string HtmlDataUri(const std::string& body);
cef_color_t ParseGuestBackgroundColor(const std::string& value,
                                      cef_color_t fallback);
double GuestZoomLevel(double factor);

#endif
