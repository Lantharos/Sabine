#include "osr/handler.h"

#include <iostream>
#include <string>
#include <utility>
#include <vector>

#include "common/bridge_policy.h"
#include "include/cef_request_context_handler.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/accelerated/paint.h"
#include "osr/guest/events.h"

using namespace sabine_osr;

class SabineGuestRequestContextHandler : public CefRequestContextHandler {
 public:
  SabineGuestRequestContextHandler(CefRefPtr<SabineOsrHandler> owner,
                                   std::string partition)
      : owner_(std::move(owner)), partition_(std::move(partition)) {}

  void OnRequestContextInitialized(
      CefRefPtr<CefRequestContext> request_context) override {
    CEF_REQUIRE_UI_THREAD();
    CefRefPtr<SabineOsrHandler> owner = owner_;
    owner_ = nullptr;
    if (owner) {
      owner->GuestRequestContextInitialized(partition_, request_context);
    }
  }

 private:
  CefRefPtr<SabineOsrHandler> owner_;
  const std::string partition_;

  IMPLEMENT_REFCOUNTING(SabineGuestRequestContextHandler);
};

GuestView* SabineOsrHandler::GuestForBrowser(
    const CefRefPtr<CefBrowser>& browser) {
  if (GuestView* guest = guests_.FindByBrowser(browser)) {
    return guest;
  }
  if (browser_ && browser_->IsSame(browser)) {
    return nullptr;
  }
  return GuestAwaitingBrowser();
}

GuestView* SabineOsrHandler::GuestAwaitingBrowser() {
  GuestView* guest = guests_.Find(guest_creations_.creating_id);
  return guest && !guest->browser ? guest : nullptr;
}

std::string SabineOsrHandler::NextGuestId() {
  std::string id;
  do {
    id = "guest-" + std::to_string(++guest_creations_.serial);
  } while (guests_.Find(id));
  return id;
}

CefRefPtr<CefRequestContext> SabineOsrHandler::CreateGuestRequestContext(
    const std::string& partition) {
  CEF_REQUIRE_UI_THREAD();
  if (partition.empty()) {
    return nullptr;
  }
  const auto cached = guest_creations_.contexts.find(partition);
  if (cached != guest_creations_.contexts.end()) {
    return cached->second;
  }
  CefRequestContextSettings settings;
  const std::string cache_path =
      profile_.directory.empty()
          ? std::string()
          : sabine_profile::CachePath(profile_, GuestFolder(partition));
  if (!cache_path.empty()) {
    CefString(&settings.cache_path).FromString(cache_path);
  }
  CefRefPtr<SabineOsrHandler> self(this);
  CefRefPtr<CefRequestContext> context = CefRequestContext::CreateContext(
      settings, new SabineGuestRequestContextHandler(self, partition));
  if (context) {
    guest_creations_.contexts[partition] = context;
  }
  return context;
}

void SabineOsrHandler::CreateGuest(GuestCreateRequest request,
                                   GuestCreateCallback callback) {
  CEF_REQUIRE_UI_THREAD();
  if (!IsValidGuestId(request.id)) {
    callback(false, "guest id is not valid");
    return;
  }
  CancelPendingGuest(request.id);
  if (guests_.Find(request.id)) {
    DestroyGuest(request.id);
  }

  request.partition =
      request.partition.empty() && request.id != kSabinePopupGuestId
          ? DefaultGuestPartition(request.id)
          : request.partition;
  if (request.partition.empty()) {
    ContinueCreateGuest(request, nullptr, std::move(callback));
    return;
  }

  const auto cached = guest_creations_.contexts.find(request.partition);
  if (cached != guest_creations_.contexts.end() &&
      guest_creations_.initialized_contexts.find(request.partition) !=
          guest_creations_.initialized_contexts.end()) {
    ContinueCreateGuest(request, cached->second, std::move(callback));
    return;
  }

  const std::string partition = request.partition;
  guest_creations_.pending[partition].push_back(
      GuestCreations::Pending{std::move(request), std::move(callback)});
  if (cached != guest_creations_.contexts.end()) {
    return;
  }
  if (!CreateGuestRequestContext(partition)) {
    GuestRequestContextInitialized(partition, nullptr);
  }
}

void SabineOsrHandler::GuestRequestContextInitialized(
    const std::string& partition,
    CefRefPtr<CefRequestContext> context) {
  CEF_REQUIRE_UI_THREAD();
  auto pending = guest_creations_.pending.find(partition);
  if (pending == guest_creations_.pending.end()) {
    if (context) {
      guest_creations_.contexts[partition] = context;
      guest_creations_.initialized_contexts.insert(partition);
    }
    return;
  }

  std::vector<GuestCreations::Pending> creates = std::move(pending->second);
  guest_creations_.pending.erase(pending);
  if (!context) {
    guest_creations_.contexts.erase(partition);
    for (GuestCreations::Pending& create : creates) {
      create.callback(false, "failed to initialize the guest request context");
    }
    return;
  }

  guest_creations_.contexts[partition] = context;
  guest_creations_.initialized_contexts.insert(partition);
  for (GuestCreations::Pending& create : creates) {
    ContinueCreateGuest(create.request, context, std::move(create.callback));
  }
}

void SabineOsrHandler::ContinueCreateGuest(const GuestCreateRequest& request,
                                           CefRefPtr<CefRequestContext> context,
                                           GuestCreateCallback callback) {
  CEF_REQUIRE_UI_THREAD();

  GuestView guest;
  guest.id = request.id;
  guest.url = request.url;
  guest.bounds = request.bounds;
  guest.partition = request.partition;
  guest.visible = request.visible;
  guest.allow_bridge = request.allow_bridge;
  guest.allow_downloads = request.allow_downloads;
  guest.intercepted_shortcuts = request.intercepted_shortcuts;
  guest.intercept_horizontal_wheel = request.intercept_horizontal_wheel;
  guest.popup_policy = request.popup_policy;
  guest.pending = true;
  guests_.Insert(std::move(guest));
  guests_.Raise(request.id);
  SendGuestOrder();
  guest_creations_.creating_id = request.id;

  CefBrowserSettings settings;
  settings.windowless_frame_rate = FrameRate();
  settings.background_color = ParseGuestBackgroundColor(
      request.background_color, CefColorSetARGB(0, 0, 0, 0));

  CefWindowInfo window_info;
  window_info.SetAsWindowless(browser_ ? browser_->GetHost()->GetWindowHandle()
                                       : kNullWindowHandle);
  sabine_osr::ApplySharedTexture(&window_info, shared_texture_);

  CefRefPtr<CefDictionaryValue> extra_info = bridge_policy_->Copy(false);
  extra_info->SetBool("enabled", request.allow_bridge);
  extra_info->SetString(
      "htmlPrefix", HtmlDataUri("<!--" + sabine_bridge::UniqueToken() + "-->"));
  const std::string initial_url =
      request.html.empty()
          ? request.url
          : sabine_bridge::TrustedHtmlUrl(extra_info, request.html);
  guests_.Find(request.id)->url = initial_url;
  guests_.Find(request.id)->bridge_policy = extra_info;
  extra_info->SetString("sabineGuestId", request.id);

  CefRefPtr<CefBrowser> browser = CefBrowserHost::CreateBrowserSync(
      window_info, this, initial_url, settings, extra_info,
      context ? context : profile_.context);
  guest_creations_.creating_id.clear();

  GuestView* created = guests_.Find(request.id);
  if (!browser) {
    if (created) {
      guests_.Erase(request.id);
    }
    callback(false, "failed to create the guest browser");
    return;
  }
  if (!created) {
    browser->GetHost()->CloseBrowser(true);
    callback(false, "guest was destroyed while it was being created");
    return;
  }
  created->browser = browser;
  created->pending = false;
  ApplyGuestBounds(*created);
  ApplyGuestVisibility(*created);
  const std::string result = GuestInfoJson(*created);
  EmitGuestEvent(*created, "guest.created", result);
  callback(true, result);
}

bool SabineOsrHandler::CancelPendingGuest(const std::string& id) {
  CEF_REQUIRE_UI_THREAD();
  std::vector<GuestCreateCallback> callbacks;
  for (auto partition = guest_creations_.pending.begin();
       partition != guest_creations_.pending.end();) {
    auto& creates = partition->second;
    for (auto create = creates.begin(); create != creates.end();) {
      if (create->request.id == id) {
        callbacks.push_back(std::move(create->callback));
        create = creates.erase(create);
      } else {
        ++create;
      }
    }
    if (creates.empty()) {
      partition = guest_creations_.pending.erase(partition);
    } else {
      ++partition;
    }
  }
  for (GuestCreateCallback& callback : callbacks) {
    callback(false, "guest creation canceled");
  }
  return !callbacks.empty();
}

bool SabineOsrHandler::HasPendingGuest(const std::string& id) const {
  for (const auto& entry : guest_creations_.pending) {
    for (const GuestCreations::Pending& create : entry.second) {
      if (create.request.id == id) {
        return true;
      }
    }
  }
  return false;
}

void SabineOsrHandler::DestroyGuest(const std::string& id) {
  CEF_REQUIRE_UI_THREAD();
  const bool canceled = CancelPendingGuest(id);
  GuestView* guest = guests_.Find(id);
  if (!guest) {
    if (canceled && !IsInternalGuestId(id)) {
      EmitPrimaryEvent("guest.destroyed", GuestIdJson(id));
    }
    return;
  }
  const GuestView removed = *guest;
  guests_.Erase(id);
  if (id == kSabineDevToolsGuestId) {
    devtools_inspected_ = nullptr;
    devtools_registration_ = nullptr;
  }
  const bool restore_primary_focus = focused_guest_id_ == id;
  if (focused_guest_id_ == id) {
    focused_guest_id_.clear();
  }
  downloads_->CancelGuest(id);
  SendGuestHidden(removed);
  if (removed.browser) {
    removed.browser->GetHost()->CloseBrowser(true);
  }
  if (restore_primary_focus && browser_) {
    browser_->GetHost()->SetFocus(true);
    SendFocusedImeState();
  }
  EmitGuestEvent(removed, "guest.destroyed", GuestIdJson(id));
  if (id == kSabinePopupGuestId) {
    EmitBridgeEvent("\"popup.close\"", "{}");
  }
}

void SabineOsrHandler::FocusGuest(const std::string& id) {
  CEF_REQUIRE_UI_THREAD();
  if (!id.empty() && !guests_.Find(id)) {
    return;
  }
  if (focused_guest_id_ != id) {
    if (GuestView* previous = guests_.Find(focused_guest_id_)) {
      if (previous->browser) {
        previous->browser->GetHost()->SetFocus(false);
      }
    }
  }
  focused_guest_id_ = id;
  if (id.empty()) {
    if (browser_) {
      browser_->GetHost()->SetFocus(true);
    }
    SendFocusedImeState();
    return;
  }
  guests_.Raise(id);
  SendGuestOrder();
  if (browser_) {
    browser_->GetHost()->SetFocus(false);
  }
  GuestView* guest = guests_.Find(id);
  if (guest && guest->browser) {
    guest->browser->GetHost()->SetFocus(true);
  }
  SendFocusedImeState();
}

void SabineOsrHandler::DismissPopupGuest() {
  if (guests_.Find(kSabinePopupGuestId)) {
    DestroyGuest(kSabinePopupGuestId);
  }
}

void SabineOsrHandler::ApplyGuestBounds(GuestView& guest) {
  if (!guest.browser) {
    return;
  }
  CefRefPtr<CefBrowserHost> host = guest.browser->GetHost();
  host->NotifyScreenInfoChanged();
  host->WasResized();
  host->Invalidate(PET_VIEW);
}

void SabineOsrHandler::ApplyGuestVisibility(GuestView& guest) {
  if (!guest.browser) {
    return;
  }
  CefRefPtr<CefBrowserHost> host = guest.browser->GetHost();
  const bool hidden = !guest.visible || view_hidden_ || guests_.Covered();
  host->WasHidden(hidden);
  if (!hidden) {
    host->Invalidate(PET_VIEW);
    return;
  }
  if (!guest.visible) {
    SendGuestHidden(guest);
  }
  if (!guest.visible && focused_guest_id_ == guest.id) {
    host->SetFocus(false);
    focused_guest_id_.clear();
    if (browser_) {
      browser_->GetHost()->SetFocus(true);
    }
  }
}

void SabineOsrHandler::ApplyGuestLifecycle() {
  CEF_REQUIRE_UI_THREAD();
  const int frame_rate = FrameRate();
  for (GuestView* guest : guests_.InZOrder()) {
    if (!guest->browser) {
      continue;
    }
    guest->browser->GetHost()->SetWindowlessFrameRate(frame_rate);
    ApplyGuestVisibility(*guest);
  }
}

void SabineOsrHandler::NotifyGuestScreenInfo() {
  CEF_REQUIRE_UI_THREAD();
  if (GuestView* devtools = guests_.Find(kSabineDevToolsGuestId)) {
    devtools->bounds = DevToolsBounds();
  }
  for (GuestView* guest : guests_.InZOrder()) {
    ApplyGuestBounds(*guest);
  }
}

void SabineOsrHandler::EmitGuestEvent(const GuestView& guest,
                                      const std::string& name,
                                      const std::string& payload) {
  if (guest.ShownToPage()) {
    EmitPrimaryEvent(name, payload);
  }
}

bool SabineOsrHandler::SendGuestPaint(const GuestView& guest,
                                      const void* buffer,
                                      int width,
                                      int height,
                                      const RectList& dirty_rects) {
  return SendPaintBatch(PaintSurface::kGuest, guest.id, guest.bounds.x,
                        guest.bounds.y, buffer, width, height, dirty_rects);
}

void SabineOsrHandler::SendGuestHidden(const GuestView& guest) {
  SendMessage(kGuestHidden, 0, 0, 0, 0, guest.id.data(),
              static_cast<uint32_t>(guest.id.size()));
  if (guest.id == kSabinePopupGuestId) {
    SendMessage(kPopupHidden, 0, 0, 0, 0, nullptr, 0);
  }
}

bool SabineOsrHandler::OnBeforePopup(
    CefRefPtr<CefBrowser> browser,
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
    bool* no_javascript_access) {
  CEF_REQUIRE_UI_THREAD();
  GuestView* guest = GuestForBrowser(browser);
  if (!guest) {
    return true;
  }
  const std::string url = target_url.ToString();
  const int disposition = static_cast<int>(target_disposition);
  switch (guest->popup_policy) {
    case GuestPopupPolicy::kNavigateSame:
      if (!url.empty()) {
        guest->url = url;
        browser->GetMainFrame()->LoadURL(url);
      }
      return true;
    case GuestPopupPolicy::kOpenGuest: {
      if (url.empty()) {
        return true;
      }
      GuestCreateRequest request;
      request.id = NextGuestId();
      request.url = url;
      request.bounds = guest->bounds;
      request.partition = guest->partition;
      request.allow_downloads = guest->allow_downloads;
      request.popup_policy = guest->popup_policy;
      CreateGuest(
          std::move(request), [](bool success, const std::string& result) {
            if (!success) {
              std::cerr << "guest popup failed: " << result << std::endl;
            }
          });
      return true;
    }
    case GuestPopupPolicy::kDeny:
    default:
      EmitGuestEvent(*guest, "guest.newWindow",
                     GuestNewWindowJson(guest->id, url, disposition));
      return true;
  }
}
