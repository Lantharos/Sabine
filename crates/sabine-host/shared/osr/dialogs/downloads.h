#ifndef SABINE_CEF_HOST_OSR_DIALOGS_DOWNLOADS_H_
#define SABINE_CEF_HOST_OSR_DIALOGS_DOWNLOADS_H_

#include <map>
#include <string>

#include "common/json.h"
#include "include/cef_download_handler.h"

class SabineOsrHandler;

// The window's downloads. The app's page hears about every one, and guests'
// downloads wait for the page to accept them.
class Downloads : public CefDownloadHandler {
 public:
  explicit Downloads(SabineOsrHandler* owner) : owner_(owner) {}

  bool OnBeforeDownload(CefRefPtr<CefBrowser> browser,
                        CefRefPtr<CefDownloadItem> download_item,
                        const CefString& suggested_name,
                        CefRefPtr<CefBeforeDownloadCallback> callback) override;
  void OnDownloadUpdated(CefRefPtr<CefBrowser> browser,
                         CefRefPtr<CefDownloadItem> download_item,
                         CefRefPtr<CefDownloadItemCallback> callback) override;

  bool RunGuestAction(const JsonObject& payload, std::string* error);
  void CancelGuest(const std::string& guest_id);

 private:
  struct Download {
    std::string guest_id;
    std::string filename;
    CefRefPtr<CefBeforeDownloadCallback> before_callback;
    CefRefPtr<CefDownloadItemCallback> item_callback;
  };

  SabineOsrHandler* const owner_;
  std::map<std::string, Download> downloads_;

  IMPLEMENT_REFCOUNTING(Downloads);
};

#endif
