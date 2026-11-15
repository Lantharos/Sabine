#ifndef SABINE_CEF_HOST_OSR_HANDLER_H_
#define SABINE_CEF_HOST_OSR_HANDLER_H_

#include <chrono>
#include <cstdint>
#include <deque>
#include <list>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "app/profile.h"
#include "include/cef_accessibility_handler.h"
#include "include/base/cef_build.h"
#include "include/cef_client.h"
#include "include/cef_command_line.h"
#include "include/cef_display_handler.h"
#include "include/cef_frame_handler.h"
#include "include/cef_keyboard_handler.h"
#include "include/cef_permission_handler.h"
#include "include/cef_render_handler.h"
#include "include/cef_request_context.h"
#include "include/cef_request_handler.h"
#include "include/cef_values.h"
#include "osr/accelerated/paint.h"
#include "osr/accessibility/page_accessibility.h"
#include "osr/dialogs/dialogs.h"
#include "osr/dialogs/downloads.h"
#include "osr/guest/guests.h"
#include "osr/input/drag.h"
#include "osr/input/ime.h"
#include "osr/input/intercept.h"
#include "osr/page/clipboard.h"
#include "osr/page/media_state.h"
#include "osr/page/permissions.h"
#include "osr/paint/dropped.h"
#include "osr/paint/software.h"
#include "osr/print/document.h"
#include "osr/transport/control_queue.h"
#include "osr/transport/message_kinds.h"
#include "osr/transport/transport.h"
#include "osr/transport/wire.h"
#include "osr/window_state.h"
#if defined(OS_LINUX)
#include "osr/print/linux/dialogs.h"
#endif

// The page sizes and frame rates a window's browser starts with.
struct BrowserViewport {
  int width = 1;
  int height = 1;
  float scale = 1.0f;
  int active_frame_rate = 60;
  int background_frame_rate = 5;
};

class SabineOsrHandler : public CefClient,
                         public CefDisplayHandler,
                         public CefDragHandler,
                         public CefFrameHandler,
                         public CefKeyboardHandler,
                         public CefLifeSpanHandler,
                         public CefLoadHandler,
                         public CefPermissionHandler,
                         public CefRenderHandler,
                         public CefRequestHandler {
 public:
  SabineOsrHandler(std::string endpoint,
                   std::string authentication_token,
                   const BrowserViewport& viewport,
                   CefRefPtr<CefDictionaryValue> bridge_policy,
                   bool dev_mode,
                   bool transparent_background,
                   bool retain_hidden_frame);
  ~SabineOsrHandler() override;

  CefRefPtr<CefContextMenuHandler> GetContextMenuHandler() override {
    return dialogs_;
  }
  CefRefPtr<CefDialogHandler> GetDialogHandler() override { return dialogs_; }
  CefRefPtr<CefJSDialogHandler> GetJSDialogHandler() override {
    return dialogs_;
  }
  CefRefPtr<CefDisplayHandler> GetDisplayHandler() override { return this; }
  CefRefPtr<CefDownloadHandler> GetDownloadHandler() override {
    return downloads_;
  }
  CefRefPtr<CefDragHandler> GetDragHandler() override { return this; }
  CefRefPtr<CefFrameHandler> GetFrameHandler() override { return this; }
  CefRefPtr<CefKeyboardHandler> GetKeyboardHandler() override { return this; }
  bool OnKeyEvent(CefRefPtr<CefBrowser> browser,
                  const CefKeyEvent& event,
                  CefEventHandle os_event) override;
  CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
  CefRefPtr<CefLoadHandler> GetLoadHandler() override { return this; }
  CefRefPtr<CefPermissionHandler> GetPermissionHandler() override {
    return this;
  }
  CefRefPtr<CefRenderHandler> GetRenderHandler() override { return this; }
  CefRefPtr<CefAccessibilityHandler> GetAccessibilityHandler() override {
    return accessibility_;
  }
  CefRefPtr<CefRequestHandler> GetRequestHandler() override { return this; }
#if defined(OS_LINUX)
  CefRefPtr<CefPrintHandler> GetPrintHandler() override {
    return print_dialogs_;
  }
#endif
  bool OnRequestMediaAccessPermission(
      CefRefPtr<CefBrowser> browser,
      CefRefPtr<CefFrame> frame,
      const CefString& requesting_origin,
      uint32_t requested_permissions,
      CefRefPtr<CefMediaAccessCallback> callback) override;
  bool OnShowPermissionPrompt(
      CefRefPtr<CefBrowser> browser,
      uint64_t prompt_id,
      const CefString& requesting_origin,
      uint32_t requested_permissions,
      CefRefPtr<CefPermissionPromptCallback> callback) override;
  void OnDismissPermissionPrompt(
      CefRefPtr<CefBrowser> browser,
      uint64_t prompt_id,
      cef_permission_request_result_t result) override;
  void OnRenderProcessTerminated(CefRefPtr<CefBrowser> browser,
                                 TerminationStatus status,
                                 int error_code,
                                 const CefString& error_string) override;
  void OnFrameDetached(CefRefPtr<CefBrowser> browser,
                       CefRefPtr<CefFrame> frame) override;
  bool OnProcessMessageReceived(CefRefPtr<CefBrowser> browser,
                                CefRefPtr<CefFrame> frame,
                                CefProcessId source_process,
                                CefRefPtr<CefProcessMessage> message) override;

  bool OnCursorChange(CefRefPtr<CefBrowser> browser,
                      CefCursorHandle cursor,
                      cef_cursor_type_t type,
                      const CefCursorInfo& custom_cursor_info) override;
  bool OnTooltip(CefRefPtr<CefBrowser> browser, CefString& text) override;
  void OnMediaAccessChange(CefRefPtr<CefBrowser> browser,
                           bool has_video_access,
                           bool has_audio_access) override;
  void OnTitleChange(CefRefPtr<CefBrowser> browser,
                     const CefString& title) override;
  bool OnConsoleMessage(CefRefPtr<CefBrowser> browser,
                        cef_log_severity_t level,
                        const CefString& message,
                        const CefString& source,
                        int line) override;
  void OnAddressChange(CefRefPtr<CefBrowser> browser,
                       CefRefPtr<CefFrame> frame,
                       const CefString& url) override;
  void OnFaviconURLChange(CefRefPtr<CefBrowser> browser,
                          const std::vector<CefString>& icon_urls) override;
  void OnAfterCreated(CefRefPtr<CefBrowser> browser) override;
  bool OnBeforePopup(CefRefPtr<CefBrowser> browser,
                     CefRefPtr<CefFrame> frame,
                     int popup_id,
                     const CefString& target_url,
                     const CefString& target_frame_name,
                     cef_window_open_disposition_t target_disposition,
                     bool user_gesture,
                     const CefPopupFeatures& popup_features,
                     CefWindowInfo& window_info,
                     CefRefPtr<CefClient>& client,
                     CefBrowserSettings& settings,
                     CefRefPtr<CefDictionaryValue>& extra_info,
                     bool* no_javascript_access) override;
  bool DoClose(CefRefPtr<CefBrowser> browser) override;
  void OnBeforeClose(CefRefPtr<CefBrowser> browser) override;
  void OnLoadError(CefRefPtr<CefBrowser> browser,
                   CefRefPtr<CefFrame> frame,
                   ErrorCode errorCode,
                   const CefString& errorText,
                   const CefString& failedUrl) override;
  void OnLoadStart(CefRefPtr<CefBrowser> browser,
                   CefRefPtr<CefFrame> frame,
                   TransitionType transition_type) override;
  void OnLoadEnd(CefRefPtr<CefBrowser> browser,
                 CefRefPtr<CefFrame> frame,
                 int httpStatusCode) override;
  void OnLoadingStateChange(CefRefPtr<CefBrowser> browser,
                            bool isLoading,
                            bool canGoBack,
                            bool canGoForward) override;
  void OnDraggableRegionsChanged(
      CefRefPtr<CefBrowser> browser,
      CefRefPtr<CefFrame> frame,
      const std::vector<CefDraggableRegion>& regions) override;

  bool GetScreenInfo(CefRefPtr<CefBrowser> browser,
                     CefScreenInfo& screen_info) override;
  void GetViewRect(CefRefPtr<CefBrowser> browser, CefRect& rect) override;
  void OnPopupShow(CefRefPtr<CefBrowser> browser, bool show) override;
  void OnPopupSize(CefRefPtr<CefBrowser> browser, const CefRect& rect) override;
  void OnPaint(CefRefPtr<CefBrowser> browser,
               PaintElementType type,
               const RectList& dirtyRects,
               const void* buffer,
               int width,
               int height) override;
  void OnAcceleratedPaint(CefRefPtr<CefBrowser> browser,
                          PaintElementType type,
                          const RectList& dirtyRects,
                          const CefAcceleratedPaintInfo& info) override;
  void OnImeCompositionRangeChanged(CefRefPtr<CefBrowser> browser,
                                    const CefRange& selected_range,
                                    const RectList& character_bounds) override;
  void OnVirtualKeyboardRequested(CefRefPtr<CefBrowser> browser,
                                  TextInputMode input_mode) override;
  bool GetScreenPoint(CefRefPtr<CefBrowser> browser,
                      int viewX,
                      int viewY,
                      int& screenX,
                      int& screenY) override;
  void SetScreenOrigin(int x, int y);
  bool StartDragging(CefRefPtr<CefBrowser> browser,
                     CefRefPtr<CefDragData> drag_data,
                     cef_drag_operations_mask_t allowed_ops,
                     int x,
                     int y) override;
  void UpdateDragCursor(CefRefPtr<CefBrowser> browser,
                        DragOperation operation) override;

  void HandleControlLine(const std::string& line,
                         const std::optional<std::string>& body = std::nullopt);
  void HandleQueuedControl(const std::string& line,
                           const std::optional<std::string>& body);
  void CompleteQueuedControl(size_t bytes);
  void HandleLatestControl(CefRefPtr<sabine_osr::LatestControl> control);
  void HandlePendingResize();
  void CloseFromNativeDisconnect();
  void ResolveBridgeResponse(
      const std::string& browser_id,
      const std::string& request_id,
      bool ok,
      const std::string& payload,
      const std::optional<std::string>& body = std::nullopt);
  void EmitBridgeEvent(const std::string& name_json,
                       const std::string& payload,
                       const std::optional<std::string>& body = std::nullopt);
  void UseProfile(BrowserProfile profile, bool shared_texture);
  void RetireAcceleratedResources(const std::vector<uint64_t>& resource_ids) {
    accelerated_.RetireResources(resource_ids);
  }
  void RepaintAll();
  void OpenDevToolsFrontend(const std::string& port, const std::string& target);
#if defined(OS_MAC)
  void UseSurfaceService(const std::string& service_name) {
    accelerated_.UseSurfaceService(service_name);
  }
#endif
#if defined(OS_WIN)
  void UseParentProcess(int pid) { transport_.parent_process().Open(pid); }
#endif
  void UseAcceleratedPaint(bool enabled) { accelerated_.Enable(enabled); }

 private:
  using BrowserList = std::list<CefRefPtr<CefBrowser>>;

  friend class Downloads;
  friend class Guests;
  friend class PageDialogs;
#if defined(OS_LINUX)
  friend class PrintDialogs;
#endif
  friend class sabine_osr::AcceleratedPaint;

  std::optional<sabine_osr::AcceleratedTarget> AcceleratedTargetFor(
      CefRefPtr<CefBrowser> browser,
      PaintElementType type);
  bool FitsView(int pixel_width, int pixel_height) const;
  bool QualifyResizeFrame(int pixel_width, int pixel_height);
  void CompleteResizeFrame(int pixel_width, int pixel_height);
  void ReleaseSharedPaint(uint32_t slot, uint32_t generation);
  void ReleaseAcceleratedFrame(uint64_t slot_token);

  bool TryHandleDragControl(const std::vector<std::string>& parts);
  void RunDragControl(const std::vector<std::string>& parts);
  void KeepDraggingOver(const CefMouseEvent& event);
  void StopDraggingOver();
  void RepeatDragOver(uint64_t serial);
  bool SendDroppedPaths(const std::string& content);
  void DroppedPathsDelivered(const std::string& serial);
  void EndDragSource(int x, int y, cef_drag_operations_mask_t operation);

  void ShowDevTools(CefRefPtr<CefBrowser> inspected);
  void CloseDevTools();
  CefRect DevToolsBounds() const;
  void OpenPrintDocument(const std::vector<std::string>& parts);
  void ReportPrintDocument(uint32_t id, bool ok);
  void PrintDocumentClosed(uint32_t id);
  void ClosePrintDocuments();
  void QuitWhenClosed();

  void HandleBridgeCommand(CefRefPtr<CefBrowser> browser,
                           CefRefPtr<CefFrame> frame,
                           const std::string& request_id,
                           const std::string& command,
                           const std::string& payload,
                           std::optional<std::string_view> body);
  void ReceiveNativeMessage(CefRefPtr<CefBrowser> browser,
                            CefRefPtr<CefFrame> frame,
                            const std::vector<std::string>& values,
                            std::optional<std::string_view> body);
  void HandleWindowCommand(const std::string& command,
                           const std::string& value);
  void ApplyHostControl(const std::string& command, const std::string& value);
  void RunMenuEdit(CefRefPtr<CefBrowser> browser, const std::string& command);
  void UpdateImeSurrounding(CefRefPtr<CefBrowser> browser,
                            CefRefPtr<CefFrame> frame,
                            const std::string& state);
  void SendFocusedImeState();
  void RequestNativeClose();
  void InstallTransparentBackground(CefRefPtr<CefFrame> frame);
  CefRefPtr<CefDictionaryValue> BridgePolicyFor(CefRefPtr<CefBrowser> browser);
  void AskPermissions(CefRefPtr<CefBrowser> browser,
                      uint64_t prompt_id,
                      const CefString& requesting_origin,
                      std::vector<std::string> requested,
                      PagePermissions::Answer answer);

  void ApplyLifecycle(const std::string& state,
                      int frame_rate,
                      const std::string& reason);
  void ApplyWindowState(bool shown, bool occluded, bool suspended);
  void SetPagesFrozen(bool frozen);
  void ReportPageZoom();
  bool UpdateViewHidden();
  void DispatchWindowState();
  void ApplyToplevelState(bool maximized, bool fullscreen, bool tiled);
  void DispatchToplevelState();
  int FrameRate() const {
    return suspended_ ? background_frame_rate_ : active_frame_rate_;
  }

  void EmitPrimaryEvent(const std::string& name, const std::string& payload);

  sabine_osr::Transport transport_;
  sabine_osr::DroppedPaints dropped_paints_;
  sabine_osr::SoftwarePaint software_paint_;
  sabine_osr::AcceleratedPaint accelerated_;
  Guests guests_;

  BrowserList browsers_;
  CefRefPtr<CefBrowser> browser_;
  int width_;
  int height_;
  float scale_;
  const int active_frame_rate_;
  const int background_frame_rate_;
  int last_main_paint_width_ = 0;
  int last_main_paint_height_ = 0;
  CefRect popup_rect_;
  int screen_origin_x_ = 0;
  int screen_origin_y_ = 0;
  double page_zoom_ = 1.0;

  CefRefPtr<CefBrowser> devtools_inspected_;
  CefRefPtr<CefRegistration> devtools_registration_;

  std::vector<KeyChord> menu_accelerators_;
  std::set<std::string> bridge_commands_;
  CefRefPtr<CefDictionaryValue> bridge_policy_;
  ImeState ime_;
  ClipboardRequests clipboard_;
  MediaState media_;
  CefRefPtr<PageAccessibility> accessibility_;
  PagePermissions permissions_;
  DragSession drag_;
  CefRefPtr<PageDialogs> dialogs_;
  CefRefPtr<Downloads> downloads_;
#if defined(OS_LINUX)
  CefRefPtr<PrintDialogs> print_dialogs_;
#endif
  std::map<uint32_t, CefRefPtr<sabine_osr::PrintDocument>> print_documents_;
  std::map<int, std::deque<std::chrono::steady_clock::time_point>>
      renderer_crashes_;

  WindowState window_;
  // Blur only lowers the frame rate: WasHidden blanks the view until the next
  // paint, which flickers after an interactive move.
  bool view_hidden_ = false;
  bool resume_needs_paint_ = false;
  bool suspended_ = false;
  bool frozen_ = false;
  bool closing_ = false;
  bool close_requested_ = false;
  const bool transparent_background_;
  const bool retain_hidden_frame_;
  const bool dev_mode_;
  BrowserProfile profile_;
  bool shared_texture_ = false;

  IMPLEMENT_REFCOUNTING(SabineOsrHandler);
};

bool CreateSabineOsrBrowser(CefRefPtr<CefCommandLine> command_line,
                            const BrowserProfile& profile);

#endif
