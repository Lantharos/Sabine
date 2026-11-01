#include "osr/guest/events.h"

#include <sstream>

#include "common/json.h"
#include "osr/guest/input.h"

namespace {

std::string BoolLiteral(bool value) {
  return value ? "true" : "false";
}

std::string CurrentUrl(const GuestView& guest) {
  if (!guest.browser) {
    return guest.url;
  }
  CefRefPtr<CefFrame> frame = guest.browser->GetMainFrame();
  const std::string url = frame ? frame->GetURL().ToString() : std::string();
  return url.empty() ? guest.url : url;
}

const char* InterruptReasonName(cef_download_interrupt_reason_t reason) {
  switch (reason) {
    case CEF_DOWNLOAD_INTERRUPT_REASON_FILE_ACCESS_DENIED:
      return "fileAccessDenied";
    case CEF_DOWNLOAD_INTERRUPT_REASON_FILE_NO_SPACE:
      return "fileNoSpace";
    case CEF_DOWNLOAD_INTERRUPT_REASON_FILE_NAME_TOO_LONG:
      return "fileNameTooLong";
    case CEF_DOWNLOAD_INTERRUPT_REASON_FILE_TOO_LARGE:
      return "fileTooLarge";
    case CEF_DOWNLOAD_INTERRUPT_REASON_FILE_VIRUS_INFECTED:
      return "fileVirusInfected";
    case CEF_DOWNLOAD_INTERRUPT_REASON_FILE_BLOCKED:
      return "fileBlocked";
    case CEF_DOWNLOAD_INTERRUPT_REASON_FILE_SECURITY_CHECK_FAILED:
      return "fileSecurityCheckFailed";
    case CEF_DOWNLOAD_INTERRUPT_REASON_NETWORK_FAILED:
      return "networkFailed";
    case CEF_DOWNLOAD_INTERRUPT_REASON_NETWORK_TIMEOUT:
      return "networkTimeout";
    case CEF_DOWNLOAD_INTERRUPT_REASON_NETWORK_DISCONNECTED:
      return "networkDisconnected";
    case CEF_DOWNLOAD_INTERRUPT_REASON_NETWORK_SERVER_DOWN:
      return "networkServerDown";
    case CEF_DOWNLOAD_INTERRUPT_REASON_SERVER_UNAUTHORIZED:
      return "serverUnauthorized";
    case CEF_DOWNLOAD_INTERRUPT_REASON_SERVER_CERT_PROBLEM:
      return "serverCertProblem";
    case CEF_DOWNLOAD_INTERRUPT_REASON_SERVER_FORBIDDEN:
      return "serverForbidden";
    case CEF_DOWNLOAD_INTERRUPT_REASON_SERVER_UNREACHABLE:
      return "serverUnreachable";
    case CEF_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED:
      return "canceled";
    case CEF_DOWNLOAD_INTERRUPT_REASON_USER_SHUTDOWN:
      return "shutdown";
    case CEF_DOWNLOAD_INTERRUPT_REASON_CRASH:
      return "crash";
    default:
      break;
  }
  if (reason >= CEF_DOWNLOAD_INTERRUPT_REASON_SERVER_FAILED)
    return "serverFailed";
  if (reason >= CEF_DOWNLOAD_INTERRUPT_REASON_NETWORK_FAILED)
    return "networkFailed";
  return "fileFailed";
}

std::string DownloadError(CefRefPtr<CefDownloadItem> item,
                          const std::string& state) {
  if (state != "interrupted" || !item || !item->IsValid())
    return std::string();
  return std::string(",\"error\":\"") +
         InterruptReasonName(item->GetInterruptReason()) + "\"";
}

}  // namespace

std::string GuestInfoJson(const GuestView& guest) {
  const bool loading =
      guest.browser ? guest.browser->IsLoading() : guest.loading;
  const bool can_go_back = guest.browser && guest.browser->CanGoBack();
  const bool can_go_forward = guest.browser && guest.browser->CanGoForward();
  std::ostringstream output;
  output << "{\"id\":\"" << JsonEscape(guest.id) << "\",\"url\":\""
         << JsonEscape(CurrentUrl(guest)) << "\",\"title\":\""
         << JsonEscape(guest.title) << "\",\"bounds\":{\"x\":" << guest.bounds.x
         << ",\"y\":" << guest.bounds.y << ",\"width\":" << guest.bounds.width
         << ",\"height\":" << guest.bounds.height
         << "},\"visible\":" << BoolLiteral(guest.visible)
         << ",\"loading\":" << BoolLiteral(loading)
         << ",\"canGoBack\":" << BoolLiteral(can_go_back)
         << ",\"canGoForward\":" << BoolLiteral(can_go_forward)
         << ",\"partition\":\"" << JsonEscape(guest.partition)
         << "\",\"allowBridge\":" << BoolLiteral(guest.allow_bridge)
         << ",\"popupPolicy\":\"" << GuestPopupPolicyName(guest.popup_policy)
         << "\",\"zoomFactor\":" << guest.zoom << "}";
  return output.str();
}

std::string GuestListJson(const std::vector<const GuestView*>& guests) {
  std::string output = "{\"guests\":[";
  bool first = true;
  for (const GuestView* guest : guests) {
    if (!first) {
      output += ",";
    }
    first = false;
    output += GuestInfoJson(*guest);
  }
  output += "]}";
  return output;
}

std::string GuestIdJson(const std::string& id) {
  return "{\"id\":\"" + JsonEscape(id) + "\"}";
}

std::string GuestNavigatedJson(const GuestView& guest) {
  std::ostringstream output;
  output << "{\"id\":\"" << JsonEscape(guest.id) << "\",\"url\":\""
         << JsonEscape(CurrentUrl(guest)) << "\",\"title\":\""
         << JsonEscape(guest.title) << "\",\"canGoBack\":"
         << BoolLiteral(guest.browser && guest.browser->CanGoBack())
         << ",\"canGoForward\":"
         << BoolLiteral(guest.browser && guest.browser->CanGoForward()) << "}";
  return output.str();
}

std::string GuestNewWindowJson(const std::string& id,
                               const std::string& url,
                               int disposition) {
  std::ostringstream output;
  output << "{\"id\":\"" << JsonEscape(id) << "\",\"url\":\"" << JsonEscape(url)
         << "\",\"disposition\":\"" << WindowOpenDispositionName(disposition)
         << "\"}";
  return output.str();
}

std::string GuestDownloadJson(const std::string& guest_id,
                              const std::string& download_id,
                              CefRefPtr<CefDownloadItem> item,
                              const std::string& state,
                              const std::string& filename) {
  const bool valid = item && item->IsValid();
  std::ostringstream output;
  output << "{\"guestId\":\"" << JsonEscape(guest_id) << "\",\"downloadId\":\""
         << JsonEscape(download_id) << "\",\"url\":\""
         << JsonEscape(valid ? item->GetURL().ToString() : std::string())
         << "\",\"filename\":\"" << JsonEscape(filename) << "\",\"mimeType\":\""
         << JsonEscape(valid ? item->GetMimeType().ToString() : std::string())
         << "\",\"totalBytes\":" << (valid ? item->GetTotalBytes() : 0)
         << ",\"receivedBytes\":" << (valid ? item->GetReceivedBytes() : 0)
         << ",\"state\":\"" << state << "\",\"savePath\":\""
         << JsonEscape(valid ? item->GetFullPath().ToString() : std::string())
         << "\"" << DownloadError(item, state) << "}";
  return output.str();
}

std::string GuestShortcutJson(const std::string& id,
                              const std::string& accelerator,
                              const std::string& key,
                              bool repeat,
                              uint32_t modifiers) {
  std::ostringstream output;
  output << "{\"id\":\"" << JsonEscape(id) << "\",\"accelerator\":\""
         << JsonEscape(accelerator) << "\",\"key\":\"" << JsonEscape(key)
         << "\",\"repeat\":" << BoolLiteral(repeat)
         << ",\"ctrlKey\":" << BoolLiteral((modifiers & kGuestModControl) != 0)
         << ",\"metaKey\":" << BoolLiteral((modifiers & kGuestModCommand) != 0)
         << ",\"altKey\":" << BoolLiteral((modifiers & kGuestModAlt) != 0)
         << ",\"shiftKey\":" << BoolLiteral((modifiers & kGuestModShift) != 0)
         << "}";
  return output.str();
}

std::string GuestWheelJson(const std::string& id,
                           double delta_x,
                           double delta_y,
                           uint32_t modifiers) {
  std::ostringstream output;
  output << "{\"id\":\"" << JsonEscape(id) << "\",\"deltaX\":" << delta_x
         << ",\"deltaY\":" << delta_y
         << ",\"ctrlKey\":" << BoolLiteral((modifiers & kGuestModControl) != 0)
         << ",\"metaKey\":" << BoolLiteral((modifiers & kGuestModCommand) != 0)
         << ",\"altKey\":" << BoolLiteral((modifiers & kGuestModAlt) != 0)
         << ",\"shiftKey\":" << BoolLiteral((modifiers & kGuestModShift) != 0)
         << "}";
  return output.str();
}

std::string GuestFaviconJson(const std::string& id,
                             const std::vector<std::string>& favicons) {
  std::ostringstream output;
  output << "{\"id\":\"" << JsonEscape(id) << "\",\"favicons\":[";
  for (size_t i = 0; i < favicons.size(); ++i) {
    if (i > 0) {
      output << ',';
    }
    output << '"' << JsonEscape(favicons[i]) << '"';
  }
  output << "]}";
  return output.str();
}
