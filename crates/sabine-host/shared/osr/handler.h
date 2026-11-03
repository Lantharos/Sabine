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
#include "include/base/cef_build.h"
#include "include/cef_client.h"
#include "include/cef_command_line.h"
#include "include/cef_display_handler.h"
#include "include/cef_keyboard_handler.h"
#include "include/cef_permission_handler.h"
#include "include/cef_render_handler.h"
#include "include/cef_request_context.h"
#include "include/cef_request_handler.h"
#include "include/cef_values.h"
#include "osr/accelerated/damage.h"
#include "osr/browser/clipboard.h"
#include "osr/browser/media_state.h"
#include "osr/dialogs/dialogs.h"
#include "osr/dialogs/downloads.h"
#include "osr/guest/registry.h"
#include "osr/input/drag.h"
#include "osr/input/ime.h"
#include "osr/print/document.h"
#include "osr/transport/control_queue.h"
#include "osr/transport/message_kinds.h"
#include "osr/transport/socket.h"
#include "osr/transport/wire.h"
#include "osr/window_state.h"
#include "osr/paint/shared_pool.h"
#include "osr/accelerated/drop.h"
#if defined(OS_LINUX)
#include "osr/accelerated/linux/dmabuf_copy.h"
#include "osr/print/linux/dialogs.h"
#endif
#if defined(OS_WIN)
#include "osr/accelerated/windows/parent_process.h"
#endif
#if defined(OS_MAC)
#include <memory>

#include "osr/accelerated/macos/surface_broker.h"
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
  CefRefPtr<CefDisplayHandler> GetDisplayHandler() override { return this; }
  CefRefPtr<CefDownloadHandler> GetDownloadHandler() override {
    return downloads_;
  }
  CefRefPtr<CefDragHandler> GetDragHandler() override { return this; }
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
  CefRefPtr<CefRequestHandler> GetRequestHandler() override { return this; }
#if defined(OS_LINUX)
  CefRefPtr<CefPrintHandler> GetPrintHandler() override {
    return print_dialogs_;
  }
#endif
  bool OnShowPermissionPrompt(
      CefRefPtr<CefBrowser> browser,
      uint64_t prompt_id,
      const CefString& requesting_origin,
      uint32_t requested_permissions,
      CefRefPtr<CefPermissionPromptCallback> callback) override;
  void OnRenderProcessTerminated(CefRefPtr<CefBrowser> browser,
                                 TerminationStatus status,
                                 int error_code,
                                 const CefString& error_string) override;
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
  bool FitsView(int pixel_width, int pixel_height) const;
  bool QualifyResizeFrame(int pixel_width, int pixel_height);
  void CompleteResizeFrame(int pixel_width, int pixel_height);
  void CloseFromNativeDisconnect();
  void ApplyHostControl(const std::string& command, const std::string& value);
  void ResolveBridgeResponse(
      const std::string& browser_id,
      const std::string& request_id,
      bool ok,
      const std::string& payload,
      const std::optional<std::string>& body = std::nullopt);
  void EmitBridgeEvent(const std::string& name_json,
                       const std::string& payload,
                       const std::optional<std::string>& body = std::nullopt);
  void ReleaseSharedPaint(uint32_t slot, uint32_t generation);
  void UseProfile(BrowserProfile profile, bool shared_texture);
  void ReleaseAcceleratedFrame(uint64_t slot_token);
  void RetireAcceleratedBrowser(int browser_id);
  // Lets the window drop resources Sabine retired; resources this window was
  // never handed are ignored.
  void RetireAcceleratedResources(const std::vector<uint64_t>& resource_ids);
  void RepaintAll();
  void OpenDevToolsFrontend(const std::string& port, const std::string& target);
#if defined(OS_MAC)
  void UseSurfaceService(const std::string& service_name);
#endif
#if defined(OS_WIN)
  void UseParentProcess(int pid);
#endif
  void UseAcceleratedPaint(bool enabled);
  void WatchAcceleratedPaint();

 private:
  using BrowserList = std::list<CefRefPtr<CefBrowser>>;

  friend class Downloads;
  friend class PageDialogs;
#if defined(OS_LINUX)
  friend class PrintDialogs;
#endif
  friend class SabineGuestRequestContextHandler;

  // Accelerated paint: Chromium's shared textures, copied into frames the
  // window composites.
  struct CopiedAccelFrame {
    uint32_t width = 0;
    uint32_t height = 0;
    uint64_t resource_id = 0;
    uint32_t resource_slot = 0;
    uint64_t shared_handle = 0;
    uint64_t slot_token = 0;
  };

  // Where a surface's accelerated frames go in the window.
  struct AcceleratedTarget {
    uint32_t kind = 0;
    std::string guest_id;
    int32_t x = 0;
    int32_t y = 0;
  };

  // What a copied frame holds: its color format and the part the page fills.
  struct AcceleratedContent {
    uint32_t format = 0;
    CefRect visible;
  };

  std::optional<AcceleratedTarget> AcceleratedTargetFor(
      CefRefPtr<CefBrowser> browser,
      PaintElementType type);
  bool SendAcceleratedFrame(const AcceleratedTarget& target,
                            const CopiedAccelFrame& copied,
                            const AcceleratedContent& content);
  bool CopyAcceleratedFrame(const sabine_osr::AcceleratedSurfaceKey& surface,
                            const sabine_osr::PixelRegion& damage,
                            const CefAcceleratedPaintInfo& info,
                            CopiedAccelFrame* out);
  void DiscardAcceleratedFrame(const CopiedAccelFrame& frame);
  void ReleaseAcceleratedSlot(uint64_t slot_token);
  void RetireAcceleratedSurfaces(int browser_id);
  void SendAcceleratedRetire(const std::vector<uint64_t>& resource_ids);
  // Chromium reports damage against the capture before it, so a frame that
  // follows one Sabine never copied has to be copied whole.
  bool FollowsCopiedCapture(
      const sabine_osr::AcceleratedSurfaceKey& surface,
      const cef_accelerated_paint_info_common_t& frame) const;
  void ForgetAcceleratedCaptures(int browser_id);
  void DropAcceleratedFrame(sabine_osr::AcceleratedDrop drop);
  void ReportAcceleratedPaintUnavailable();
  void ReportMissingAcceleratedPaint();
  bool SendSharedPaint(PaintSurface surface,
                       const std::string& guest_id,
                       int32_t origin_x,
                       int32_t origin_y,
                       const void* buffer,
                       int buffer_width,
                       int buffer_height,
                       size_t total_bytes);
#if defined(OS_LINUX)
  void RepaintAllSoon();
  bool ShareDmabufCopy(sabine_osr::DmabufCopy result,
                       const sabine_osr::AccelDmabufCopiedFrame& copied,
                       CopiedAccelFrame* out);

  // A page frame shown as copied while Chromium may still have been drawing
  // it, copied again once the page stops painting.
  struct UnsettledFrame {
    CefRefPtr<CefBrowser> browser;
    AcceleratedContent content;
    std::chrono::steady_clock::time_point painted_at;
  };

  void SettleWhenPaused(const sabine_osr::AcceleratedSurfaceKey& surface,
                        CefRefPtr<CefBrowser> browser,
                        const AcceleratedContent& content);
  void ScheduleSettle(std::chrono::milliseconds delay);
  void SettlePausedFrames();
  void SettleFrame(const sabine_osr::AcceleratedSurfaceKey& surface,
                   const UnsettledFrame& frame);
#endif

  sabine_osr::SharedPaintPool shared_paint_;
  sabine_osr::PaintScratch paint_scratch_;
  std::vector<char> accel_payload_;
  // Surfaces whose frame was dropped, painted again once a slot frees up.
  std::set<sabine_osr::AcceleratedSurfaceKey> dropped_accelerated_paints_;
  std::map<sabine_osr::AcceleratedSurfaceKey, uint64_t> copied_captures_;
  std::set<uint64_t> announced_accelerated_resources_;
#if defined(OS_WIN)
  sabine_osr::ParentProcess parent_process_;
#endif
#if defined(OS_MAC)
  std::unique_ptr<sabine_osr::SurfaceBroker> surface_broker_;
#endif
  bool accelerated_paint_ = false;
  bool accelerated_paint_seen_ = false;
  bool accelerated_paint_failed_ = false;
#if defined(OS_LINUX)
  std::map<sabine_osr::AcceleratedSurfaceKey, UnsettledFrame> unsettled_frames_;
  bool settle_pending_ = false;
  bool repaint_all_pending_ = false;
#endif
  sabine_osr::AcceleratedDrop accelerated_drop_;
  // End of accelerated paint.

  void StartCommandReader();
  void ReadControls();
  bool DispatchControl(std::string line,
                       std::optional<std::string> body,
                       CefRefPtr<sabine_osr::LatestControl>* move);
  void QueueResizeControlLine(std::string line);
  void PostPendingResize();
  void CloseTransport();
  bool SendMessage(uint32_t kind,
                   uint32_t width,
                   uint32_t height,
                   int32_t x,
                   int32_t y,
                   const void* payload,
                   uint32_t payload_len);
  bool SendMessage(uint32_t kind,
                   std::initializer_list<std::string_view> parts);
#if !defined(OS_WIN)
  bool SendMessageWithFd(uint32_t kind,
                         uint32_t width,
                         uint32_t height,
                         int32_t x,
                         int32_t y,
                         const void* payload,
                         uint32_t payload_len,
                         int fd);
#endif
  bool SendPaintBatch(PaintSurface surface,
                      const std::string& guest_id,
                      int32_t origin_x,
                      int32_t origin_y,
                      const void* buffer,
                      int buffer_width,
                      int buffer_height,
                      const RectList& dirty_rects);

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
  void ForwardClipboardRequest(CefRefPtr<CefBrowser> browser,
                               CefRefPtr<CefFrame> frame,
                               const std::string& request);
  bool ResolveClipboardResponse(const std::string& request_id,
                                bool ok,
                                const std::string& payload);
  void UpdateImeSurrounding(CefRefPtr<CefBrowser> browser,
                            CefRefPtr<CefFrame> frame,
                            const std::string& state);
  void SendFocusedImeState();
  void RequestNativeClose();
  void InstallTransparentBackground(CefRefPtr<CefFrame> frame);
  CefRefPtr<CefDictionaryValue> BridgePolicyFor(CefRefPtr<CefBrowser> browser);

  void ApplyLifecycle(const std::string& state,
                      int frame_rate,
                      const std::string& reason);
  void ApplyWindowState(bool shown, bool occluded, bool suspended);
  void SetPagesFrozen(bool frozen);
  void UpdateFrameMediaState(CefRefPtr<CefBrowser> browser,
                             CefRefPtr<CefFrame> frame,
                             bool playing);
  void ReportMediaPlaying();
  void ReportPageZoom();
  bool UpdateViewHidden();
  void DispatchWindowState();
  void ApplyToplevelState(bool maximized, bool fullscreen, bool tiled);
  void DispatchToplevelState();
  int FrameRate() const {
    return suspended_ ? background_frame_rate_ : active_frame_rate_;
  }

  GuestView* GuestForBrowser(const CefRefPtr<CefBrowser>& browser);
  GuestView* GuestAwaitingBrowser();
  bool HandleGuestBridgeCommand(const std::string& command,
                                const std::string& payload,
                                const std::string& browser_id,
                                const std::string& request_id);
  bool RunGuestOperation(const std::string& operation,
                         const JsonObject& payload,
                         std::string* response,
                         std::string* error);
  void CreateGuest(GuestCreateRequest request, GuestCreateCallback callback);
  void ContinueCreateGuest(const GuestCreateRequest& request,
                           CefRefPtr<CefRequestContext> context,
                           GuestCreateCallback callback);
  void GuestRequestContextInitialized(const std::string& partition,
                                      CefRefPtr<CefRequestContext> context);
  bool CancelPendingGuest(const std::string& id);
  bool HasPendingGuest(const std::string& id) const;
  void DestroyGuest(const std::string& id);
  void FocusGuest(const std::string& id);
  void DismissPopupGuest();
  void ApplyGuestBounds(GuestView& guest);
  void ApplyGuestVisibility(GuestView& guest);
  void ApplyGuestLifecycle();
  void NotifyGuestScreenInfo();
  bool SendGuestPaint(const GuestView& guest,
                      const void* buffer,
                      int width,
                      int height,
                      const RectList& dirty_rects);
  void SendGuestHidden(const GuestView& guest);
  // Tells the window the order guests stack in, bottom first, which it draws
  // them in.
  void SendGuestOrder();
  void EmitPrimaryEvent(const std::string& name, const std::string& payload);
  void EmitGuestEvent(const GuestView& guest,
                      const std::string& name,
                      const std::string& payload);
  CefRefPtr<CefRequestContext> CreateGuestRequestContext(
      const std::string& partition);
  std::string NextGuestId();

  const std::string endpoint_;
  const std::string authentication_token_;
  sabine_osr::Socket socket_;
  sabine_osr::ControlQueue controls_;
  sabine_osr::PendingResize resize_;

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
  CefRect guest_popup_rect_;
  int screen_origin_x_ = 0;
  int screen_origin_y_ = 0;
  double page_zoom_ = 1.0;

  GuestRegistry guests_;
  GuestCreations guest_creations_;
  std::string focused_guest_id_;
  bool pending_guest_cover_ = false;
  CefRefPtr<CefBrowser> devtools_inspected_;
  CefRefPtr<CefRegistration> devtools_registration_;

  std::set<std::string> bridge_commands_;
  CefRefPtr<CefDictionaryValue> bridge_policy_;
  ImeState ime_;
  ClipboardRequests clipboard_;
  MediaState media_;
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
