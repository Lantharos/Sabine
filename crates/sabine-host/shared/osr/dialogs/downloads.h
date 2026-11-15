#ifndef SABINE_CEF_HOST_OSR_DIALOGS_DOWNLOADS_H_
#define SABINE_CEF_HOST_OSR_DIALOGS_DOWNLOADS_H_

#include <map>
#include <string>

#include "common/json.h"
#include "include/cef_download_handler.h"
#include "include/cef_values.h"

class SabineOsrHandler;

// The window's downloads. The app's page hears about every one. Its own
// downloads go where the app decides, or where people choose in the save
// dialog, and guests' downloads wait for the page to accept them.
class Downloads : public CefDownloadHandler {
 public:
  // |policy| says whether the app decides where the page's downloads go, and
  // whether it hears how they go.
  Downloads(SabineOsrHandler* owner, CefRefPtr<CefDictionaryValue> policy)
      : owner_(owner),
        app_decides_(policy->GetBool("decide")),
        app_hears_(policy->GetBool("report")) {}

  bool OnBeforeDownload(CefRefPtr<CefBrowser> browser,
                        CefRefPtr<CefDownloadItem> download_item,
                        const CefString& suggested_name,
                        CefRefPtr<CefBeforeDownloadCallback> callback) override;
  void OnDownloadUpdated(CefRefPtr<CefBrowser> browser,
                         CefRefPtr<CefDownloadItem> download_item,
                         CefRefPtr<CefDownloadItemCallback> callback) override;

  // Accepts, cancels, pauses or resumes one of the guests' downloads, or of
  // the page's when |guests| is false, as |payload| names it.
  bool RunAction(const JsonObject& payload,
                 const std::string& action,
                 bool guests,
                 std::string* error);
  // Answers the page's `sabine.download.*` calls.
  bool HandleBridgeCommand(const std::string& command,
                           const std::string& payload,
                           const std::string& browser_id,
                           const std::string& request_id);
  void CancelGuest(const std::string& guest_id);

 private:
  struct Download {
    std::string guest_id;
    std::string filename;
    CefRefPtr<CefBeforeDownloadCallback> before_callback;
    CefRefPtr<CefDownloadItemCallback> item_callback;
  };

  void Report(const std::string& download_id,
              const Download& download,
              CefRefPtr<CefDownloadItem> item,
              const std::string& state);

  SabineOsrHandler* const owner_;
  const bool app_decides_;
  const bool app_hears_;
  std::map<std::string, Download> downloads_;

  IMPLEMENT_REFCOUNTING(Downloads);
};

#endif
