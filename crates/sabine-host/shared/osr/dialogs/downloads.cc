#include "osr/dialogs/downloads.h"

#include <cstdio>
#include <sstream>
#include <string>

#include "include/wrapper/cef_helpers.h"
#include "osr/handler.h"

namespace {

constexpr size_t kMaxDownloads = 256;
constexpr char kPageCommandPrefix[] = "sabine.download.";
// The line that hands the app a download of the window's page.
constexpr char kAppReport[] = "SABINE_DOWNLOAD\t";

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

std::string DownloadJson(const std::string& guest_id,
                         const std::string& download_id,
                         CefRefPtr<CefDownloadItem> item,
                         const std::string& state,
                         const std::string& filename) {
  const bool valid = item && item->IsValid();
  std::ostringstream output;
  output << "{";
  if (!guest_id.empty()) {
    output << "\"guestId\":\"" << JsonEscape(guest_id) << "\",";
  }
  output << "\"downloadId\":\"" << JsonEscape(download_id) << "\",\"url\":\""
         << JsonEscape(valid ? item->GetURL().ToString() : std::string())
         << "\",\"filename\":\"" << JsonEscape(filename) << "\",\"mimeType\":\""
         << JsonEscape(valid ? item->GetMimeType().ToString() : std::string())
         << "\",\"totalBytes\":" << (valid ? item->GetTotalBytes() : 0)
         << ",\"receivedBytes\":" << (valid ? item->GetReceivedBytes() : 0)
         << ",\"state\":\"" << state << "\",\"savePath\":\""
         << JsonEscape(valid ? item->GetFullPath().ToString() : std::string())
         << "\"";
  if (state == "interrupted" && valid) {
    output << ",\"error\":\"" << InterruptReasonName(item->GetInterruptReason())
           << "\"";
  }
  output << "}";
  return output.str();
}

}  // namespace

bool Downloads::RunAction(const JsonObject& payload,
                          const std::string& action,
                          bool guests,
                          std::string* error) {
  CEF_REQUIRE_UI_THREAD();
  const std::string download_id = payload.String("downloadId");
  const auto entry = downloads_.find(download_id);
  if (entry == downloads_.end() || entry->second.guest_id.empty() == guests) {
    *error = "unknown downloadId";
    return false;
  }
  Download& download = entry->second;
  if (action == "accept") {
    if (!download.before_callback) {
      return true;
    }
    CefRefPtr<CefBeforeDownloadCallback> callback = download.before_callback;
    download.before_callback = nullptr;
    callback->Continue(payload.String("savePath"),
                       payload.Bool("showDialog", false));
    return true;
  }
  if (action == "cancel") {
    if (download.item_callback) {
      download.item_callback->Cancel();
    }
    downloads_.erase(entry);
    return true;
  }
  if (action == "pause" || action == "resume") {
    if (download.item_callback) {
      if (action == "pause") {
        download.item_callback->Pause();
      } else {
        download.item_callback->Resume();
      }
    }
    return true;
  }
  *error = "download `action` must be accept, cancel, pause, or resume";
  return false;
}

bool Downloads::HandleBridgeCommand(const std::string& command,
                                    const std::string& payload,
                                    const std::string& browser_id,
                                    const std::string& request_id) {
  if (command.rfind(kPageCommandPrefix, 0) != 0) {
    return false;
  }
  const std::string action = command.substr(sizeof(kPageCommandPrefix) - 1);
  std::string error = "sabine.download takes pause, resume and cancel";
  const bool ok = action != "accept" &&
                  RunAction(JsonObject::Parse(payload), action, false, &error);
  owner_->ResolveBridgeResponse(browser_id, request_id, ok,
                                ok ? "null" : JsonMessage(error));
  return true;
}

void Downloads::CancelGuest(const std::string& guest_id) {
  for (auto entry = downloads_.begin(); entry != downloads_.end();) {
    if (entry->second.guest_id != guest_id) {
      ++entry;
      continue;
    }
    if (entry->second.item_callback) {
      entry->second.item_callback->Cancel();
    }
    entry = downloads_.erase(entry);
  }
}

bool Downloads::OnBeforeDownload(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefDownloadItem> download_item,
    const CefString& suggested_name,
    CefRefPtr<CefBeforeDownloadCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  GuestView* guest = owner_->guests_.ForBrowser(browser);
  if (guest && !guest->allow_downloads) {
    return true;
  }
  if (downloads_.size() >= kMaxDownloads) {
    std::fprintf(stderr,
                 "Sabine: download canceled because this window already has "
                 "256 pending or active downloads\n");
    return true;
  }
  const std::string download_id = std::to_string(download_item->GetId());
  Download& download = downloads_[download_id];
  download.guest_id = guest ? guest->id : std::string();
  download.filename = suggested_name.ToString();
  download.before_callback = guest || app_decides_ ? callback : nullptr;
  Report(download_id, download, download_item, "requested");
  if (!download.before_callback) {
    callback->Continue(CefString(), true);
  }
  return true;
}

void Downloads::OnDownloadUpdated(CefRefPtr<CefBrowser> browser,
                                  CefRefPtr<CefDownloadItem> download_item,
                                  CefRefPtr<CefDownloadItemCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  const std::string download_id = std::to_string(download_item->GetId());
  const auto entry = downloads_.find(download_id);
  if (entry == downloads_.end()) {
    return;
  }
  entry->second.item_callback = callback;
  std::string state = "progress";
  if (download_item->IsComplete()) {
    state = "completed";
  } else if (download_item->IsCanceled()) {
    state = "cancelled";
  } else if (!download_item->IsInProgress()) {
    state = "interrupted";
  }
  Report(download_id, entry->second, download_item, state);
  if (state != "progress") {
    downloads_.erase(entry);
  }
}

void Downloads::Report(const std::string& download_id,
                       const Download& download,
                       CefRefPtr<CefDownloadItem> item,
                       const std::string& state) {
  const std::string json = DownloadJson(download.guest_id, download_id, item,
                                        state, download.filename);
  if (!download.guest_id.empty()) {
    owner_->EmitPrimaryEvent("guest.download", json);
    return;
  }
  owner_->EmitPrimaryEvent("download", json);
  if (app_hears_) {
    owner_->transport_.Send(kBridgeRequest, {kAppReport, json});
  }
}
