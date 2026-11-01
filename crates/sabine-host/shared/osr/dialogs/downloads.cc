#include "osr/dialogs/downloads.h"

#include <cstdio>
#include <string>

#include "include/wrapper/cef_helpers.h"
#include "osr/guest/events.h"
#include "osr/handler.h"

bool Downloads::RunGuestAction(const JsonObject& payload, std::string* error) {
  CEF_REQUIRE_UI_THREAD();
  const std::string download_id = payload.String("downloadId");
  if (download_id.empty()) {
    *error = "guest.downloadAction requires a `downloadId`";
    return false;
  }
  const auto entry = downloads_.find(download_id);
  if (entry == downloads_.end()) {
    *error = "unknown downloadId";
    return false;
  }
  Download& download = entry->second;
  std::string action = payload.String("action");
  if (action.empty()) {
    action = "accept";
  }
  if (action == "accept") {
    if (!download.before_callback) {
      return true;
    }
    const std::string save_path = payload.String("savePath");
    const bool show_dialog = payload.Bool("showDialog", false);
    CefRefPtr<CefBeforeDownloadCallback> callback = download.before_callback;
    download.before_callback = nullptr;
    callback->Continue(save_path, show_dialog);
    return true;
  }
  if (action == "cancel") {
    if (download.item_callback) {
      download.item_callback->Cancel();
    }
    downloads_.erase(entry);
    return true;
  }
  if (action == "pause") {
    if (download.item_callback) {
      download.item_callback->Pause();
    }
    return true;
  }
  if (action == "resume") {
    if (download.item_callback) {
      download.item_callback->Resume();
    }
    return true;
  }
  *error =
      "guest.downloadAction `action` must be accept, cancel, pause, or "
      "resume";
  return false;
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
  GuestView* guest = owner_->GuestForBrowser(browser);
  if (guest && !guest->allow_downloads) {
    return true;
  }
  if (downloads_.size() >= 256) {
    std::fprintf(stderr,
                 "Sabine: download canceled because this window already has "
                 "256 pending or active downloads\n");
    return true;
  }
  const std::string download_id = std::to_string(download_item->GetId());
  Download download;
  download.guest_id = guest ? guest->id : std::string();
  download.filename = suggested_name.ToString();
  download.before_callback = guest ? callback : nullptr;
  downloads_[download_id] = download;
  owner_->EmitPrimaryEvent(
      "guest.download",
      GuestDownloadJson(download.guest_id, download_id, download_item,
                        "requested", download.filename));
  if (!guest)
    callback->Continue(CefString(), true);
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
  owner_->EmitPrimaryEvent(
      "guest.download",
      GuestDownloadJson(entry->second.guest_id, download_id, download_item,
                        state, entry->second.filename));
  if (state != "progress") {
    downloads_.erase(entry);
  }
}
