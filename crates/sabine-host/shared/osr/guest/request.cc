#include "osr/guest/request.h"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <cstdio>
#include <cstdlib>

#include "include/cef_parser.h"

const char kGuestBridgePrefix[] = "sabine.guest.";
const char kGuestHostControlPrefix[] = "guest.";

namespace {

const char* const kGuestOperations[] = {
    "create",     "destroy",    "navigate",          "setBounds",
    "setVisible", "setCovered", "capturePreview",    "focus",
    "reload",     "goBack",     "goForward",         "list",
    "get",        "setZoom",    "executeJavaScript", "downloadAction",
};

std::string SanitizePartition(const std::string& partition) {
  std::string output;
  output.reserve(partition.size());
  for (unsigned char c : partition) {
    if (std::isalnum(c) || c == '-' || c == '_') {
      output += static_cast<char>(c);
    } else {
      char buffer[4];
      std::snprintf(buffer, sizeof(buffer), "%02x", c);
      output += buffer;
    }
  }
  return output;
}

}  // namespace

GuestPopupPolicy ParseGuestPopupPolicy(const std::string& value,
                                       GuestPopupPolicy fallback) {
  if (value == "deny") {
    return GuestPopupPolicy::kDeny;
  }
  if (value == "allow") {
    return GuestPopupPolicy::kAllow;
  }
  if (value == "navigateSame") {
    return GuestPopupPolicy::kNavigateSame;
  }
  if (value == "openGuest") {
    return GuestPopupPolicy::kOpenGuest;
  }
  return fallback;
}

const char* GuestPopupPolicyName(GuestPopupPolicy policy) {
  switch (policy) {
    case GuestPopupPolicy::kAllow:
      return "allow";
    case GuestPopupPolicy::kNavigateSame:
      return "navigateSame";
    case GuestPopupPolicy::kOpenGuest:
      return "openGuest";
    case GuestPopupPolicy::kDeny:
    default:
      return "deny";
  }
}

const char* WindowOpenDispositionName(int disposition) {
  static const char* const kNames[] = {
      "unknown",          "currentTab",   "singletonTab", "newForegroundTab",
      "newBackgroundTab", "newPopup",     "newWindow",    "saveToDisk",
      "offTheRecord",     "ignoreAction", "switchToTab",  "newPictureInPicture",
      "newSplitView",
  };
  const int count = static_cast<int>(sizeof(kNames) / sizeof(kNames[0]));
  if (disposition < 0 || disposition >= count) {
    return "unknown";
  }
  return kNames[disposition];
}

bool IsValidGuestId(const std::string& id) {
  if (id.empty() || id.size() > 128) {
    return false;
  }
  return std::all_of(id.begin(), id.end(), [](unsigned char c) {
    return std::isalnum(c) || c == '-' || c == '_' || c == '.';
  });
}

bool IsGuestBridgeCommand(const std::string& command) {
  const std::string operation = GuestOperationName(command, kGuestBridgePrefix);
  if (operation.empty()) {
    return false;
  }
  for (const char* known : kGuestOperations) {
    if (operation == known) {
      return true;
    }
  }
  return false;
}

std::string GuestOperationName(const std::string& command,
                               const std::string& prefix) {
  if (command.rfind(prefix, 0) != 0) {
    return "";
  }
  return command.substr(prefix.size());
}

std::string DefaultGuestPartition(const std::string& id) {
  return "guest:" + id;
}

std::string GuestFolder(const std::string& partition) {
  return "guest-" + SanitizePartition(partition);
}

std::string HtmlDataUri(const std::string& body) {
  return "data:text/html;base64," +
         CefBase64Encode(body.data(), body.size()).ToString();
}

cef_color_t ParseGuestBackgroundColor(const std::string& value,
                                      cef_color_t fallback) {
  std::string digits = value;
  if (!digits.empty() && digits[0] == '#') {
    digits.erase(0, 1);
  }
  if (digits.size() != 6 && digits.size() != 8) {
    return fallback;
  }
  char* end = nullptr;
  const unsigned long parsed = std::strtoul(digits.c_str(), &end, 16);
  if (end != digits.c_str() + digits.size()) {
    return fallback;
  }
  if (digits.size() == 6) {
    return static_cast<cef_color_t>(0xff000000u | parsed);
  }
  return static_cast<cef_color_t>(parsed);
}

double GuestZoomLevel(double factor) {
  const double clamped = std::min(5.0, std::max(0.25, factor));
  return std::log(clamped) / std::log(1.2);
}

CefRect ParseGuestBounds(const JsonObject& payload, const CefRect& fallback) {
  const JsonObject source = payload.Object("bounds");
  return CefRect(source.Int("x", fallback.x), source.Int("y", fallback.y),
                 std::max(1, source.Int("width", fallback.width)),
                 std::max(1, source.Int("height", fallback.height)));
}

bool ParseGuestCreateRequest(const JsonObject& payload,
                             GuestCreateRequest* request,
                             std::string* error) {
  const std::string id = payload.String("id");
  if (!id.empty()) {
    if (!IsValidGuestId(id)) {
      *error =
          "guest id may only contain ASCII alphanumerics, '-', '_', and '.'";
      return false;
    }
    request->id = id;
  }
  const std::string url = payload.String("url");
  const std::string html = payload.String("html");
  if (url.empty() && html.empty()) {
    *error = "guest.create requires a non-empty `url` or `html`";
    return false;
  }
  request->url = url.empty() ? HtmlDataUri(html) : url;
  request->html = url.empty() ? html : "";
  request->bounds = ParseGuestBounds(payload, CefRect(0, 0, 1, 1));
  request->partition = payload.String("partition");
  request->allow_bridge = payload.Bool("allowBridge", false);
  request->allow_downloads = payload.Bool("allowDownloads", true);
  request->intercepted_shortcuts = payload.Strings("interceptedShortcuts");
  request->intercept_horizontal_wheel =
      payload.Bool("interceptHorizontalWheel", false);
  request->visible = payload.Bool("visible", true);
  request->popup_policy = ParseGuestPopupPolicy(payload.String("popupPolicy"),
                                                GuestPopupPolicy::kDeny);
  request->background_color = payload.String("backgroundColor");
  return true;
}
