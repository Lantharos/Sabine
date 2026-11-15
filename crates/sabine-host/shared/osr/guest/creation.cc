#include "osr/guest/guests.h"

#include <iostream>
#include <string>
#include <utility>
#include <vector>

#include "common/bridge_policy.h"
#include "include/cef_request_context_handler.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/accelerated/paint.h"
#include "osr/guest/events.h"
#include "osr/handler.h"

namespace {

constexpr size_t kMaxPopupGuests = 16;

class GuestRequestContextHandler : public CefRequestContextHandler {
 public:
  GuestRequestContextHandler(CefRefPtr<SabineOsrHandler> owner,
                             Guests* guests,
                             std::string partition)
      : owner_(std::move(owner)),
        guests_(guests),
        partition_(std::move(partition)) {}

  void OnRequestContextInitialized(
      CefRefPtr<CefRequestContext> request_context) override {
    CEF_REQUIRE_UI_THREAD();
    CefRefPtr<SabineOsrHandler> owner = owner_;
    owner_ = nullptr;
    if (owner) {
      guests_->RequestContextInitialized(partition_, request_context);
    }
  }

 private:
  CefRefPtr<SabineOsrHandler> owner_;
  Guests* const guests_;
  const std::string partition_;

  IMPLEMENT_REFCOUNTING(GuestRequestContextHandler);
};

}  // namespace

std::string Guests::NextId() {
  std::string id;
  do {
    id = "guest-" + std::to_string(++creations_.serial);
  } while (registry_.Find(id));
  return id;
}

CefRefPtr<CefRequestContext> Guests::CreateRequestContext(
    const std::string& partition) {
  CEF_REQUIRE_UI_THREAD();
  if (partition.empty()) {
    return nullptr;
  }
  const auto cached = creations_.contexts.find(partition);
  if (cached != creations_.contexts.end()) {
    return cached->second;
  }
  CefRequestContextSettings settings;
  const std::string cache_path =
      owner_->profile_.directory.empty()
          ? std::string()
          : sabine_profile::CachePath(owner_->profile_, GuestFolder(partition));
  if (!cache_path.empty()) {
    CefString(&settings.cache_path).FromString(cache_path);
  }
  CefRefPtr<CefRequestContext> context = CefRequestContext::CreateContext(
      settings, new GuestRequestContextHandler(owner_, this, partition));
  if (context) {
    creations_.contexts[partition] = context;
  }
  return context;
}

void Guests::Create(GuestCreateRequest request, GuestCreateCallback callback) {
  CEF_REQUIRE_UI_THREAD();
  if (!IsValidGuestId(request.id)) {
    callback(false, "guest id is not valid");
    return;
  }
  CancelPending(request.id);
  if (registry_.Find(request.id)) {
    Destroy(request.id);
  }

  request.partition =
      request.partition.empty() && request.id != kSabinePopupGuestId
          ? DefaultGuestPartition(request.id)
          : request.partition;
  if (request.partition.empty()) {
    ContinueCreate(request, nullptr, std::move(callback));
    return;
  }

  const auto cached = creations_.contexts.find(request.partition);
  if (cached != creations_.contexts.end() &&
      creations_.initialized_contexts.find(request.partition) !=
          creations_.initialized_contexts.end()) {
    ContinueCreate(request, cached->second, std::move(callback));
    return;
  }

  const std::string partition = request.partition;
  creations_.pending[partition].push_back(
      GuestCreations::Pending{std::move(request), std::move(callback)});
  if (cached != creations_.contexts.end()) {
    return;
  }
  if (!CreateRequestContext(partition)) {
    RequestContextInitialized(partition, nullptr);
  }
}

void Guests::RequestContextInitialized(const std::string& partition,
                                       CefRefPtr<CefRequestContext> context) {
  CEF_REQUIRE_UI_THREAD();
  auto pending = creations_.pending.find(partition);
  if (pending == creations_.pending.end()) {
    if (context) {
      creations_.contexts[partition] = context;
      creations_.initialized_contexts.insert(partition);
    }
    return;
  }

  std::vector<GuestCreations::Pending> creates = std::move(pending->second);
  creations_.pending.erase(pending);
  if (!context) {
    creations_.contexts.erase(partition);
    for (GuestCreations::Pending& create : creates) {
      create.callback(false, "failed to initialize the guest request context");
    }
    return;
  }

  creations_.contexts[partition] = context;
  creations_.initialized_contexts.insert(partition);
  for (GuestCreations::Pending& create : creates) {
    ContinueCreate(create.request, context, std::move(create.callback));
  }
}

void Guests::ContinueCreate(const GuestCreateRequest& request,
                            CefRefPtr<CefRequestContext> context,
                            GuestCreateCallback callback) {
  CEF_REQUIRE_UI_THREAD();

  GuestView guest;
  guest.id = request.id;
  guest.url = request.url;
  guest.bounds = request.bounds;
  guest.page_zoom = guest.FollowsPageZoom() ? owner_->page_zoom_ : 1.0;
  guest.partition = request.partition;
  guest.visible = request.visible;
  guest.allow_bridge = request.allow_bridge;
  guest.allow_downloads = request.allow_downloads;
  guest.permissions = request.permissions;
  guest.intercepted_shortcuts = request.intercepted_shortcuts;
  guest.intercept_horizontal_wheel = request.intercept_horizontal_wheel;
  guest.popup_policy = request.popup_policy;
  guest.opened_by_popup = request.opened_by_popup;
  guest.pending = true;
  registry_.Insert(std::move(guest));
  registry_.Raise(request.id);
  SendOrder();
  creations_.creating_id = request.id;

  CefBrowserSettings settings;
  settings.windowless_frame_rate = owner_->FrameRate();
  settings.background_color = ParseGuestBackgroundColor(
      request.background_color, CefColorSetARGB(0, 0, 0, 0));

  CefWindowInfo window_info;
  window_info.SetAsWindowless(
      owner_->browser_ ? owner_->browser_->GetHost()->GetWindowHandle()
                       : kNullWindowHandle);
  sabine_osr::ApplySharedTexture(&window_info, owner_->shared_texture_);

  CefRefPtr<CefDictionaryValue> extra_info =
      owner_->bridge_policy_->Copy(false);
  extra_info->SetBool("enabled", request.allow_bridge);
  extra_info->SetString(
      "htmlPrefix", HtmlDataUri("<!--" + sabine_bridge::UniqueToken() + "-->"));
  const std::string initial_url =
      request.html.empty()
          ? request.url
          : sabine_bridge::TrustedHtmlUrl(extra_info, request.html);
  registry_.Find(request.id)->url = initial_url;
  registry_.Find(request.id)->bridge_policy = extra_info;
  extra_info->SetString("sabineGuestId", request.id);

  CefRefPtr<CefBrowser> browser = CefBrowserHost::CreateBrowserSync(
      window_info, owner_, initial_url, settings, extra_info,
      context ? context : owner_->profile_.context);
  creations_.creating_id.clear();

  GuestView* created = registry_.Find(request.id);
  if (!browser) {
    if (created) {
      registry_.Erase(request.id);
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
  ApplyBounds(*created);
  ApplyVisibility(*created);
  const std::string result = GuestInfoJson(*created);
  Emit(*created, "guest.created", result);
  callback(true, result);
}

bool Guests::CancelPending(const std::string& id) {
  CEF_REQUIRE_UI_THREAD();
  std::vector<GuestCreateCallback> callbacks;
  for (auto partition = creations_.pending.begin();
       partition != creations_.pending.end();) {
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
      partition = creations_.pending.erase(partition);
    } else {
      ++partition;
    }
  }
  for (GuestCreateCallback& callback : callbacks) {
    callback(false, "guest creation canceled");
  }
  return !callbacks.empty();
}

size_t Guests::PopupCount() const {
  size_t count = 0;
  for (const GuestView* guest : registry_.InZOrder()) {
    count += guest->opened_by_popup;
  }
  for (const auto& entry : creations_.pending) {
    for (const GuestCreations::Pending& create : entry.second) {
      count += create.request.opened_by_popup;
    }
  }
  return count;
}

bool Guests::HasPending(const std::string& id) const {
  for (const auto& entry : creations_.pending) {
    for (const GuestCreations::Pending& create : entry.second) {
      if (create.request.id == id) {
        return true;
      }
    }
  }
  return false;
}

void Guests::OpenPopup(CefRefPtr<CefBrowser> browser,
                       const std::string& url,
                       int disposition,
                       bool user_gesture) {
  CEF_REQUIRE_UI_THREAD();
  GuestView* guest = ForBrowser(browser);
  if (!guest) {
    return;
  }
  switch (guest->popup_policy) {
    case GuestPopupPolicy::kNavigateSame:
      if (!sabine_bridge::Origin(url).empty()) {
        guest->url = url;
        browser->GetMainFrame()->LoadURL(url);
      }
      return;
    case GuestPopupPolicy::kOpenGuest: {
      if (!user_gesture || sabine_bridge::Origin(url).empty() ||
          PopupCount() >= kMaxPopupGuests) {
        return;
      }
      GuestCreateRequest request;
      request.id = NextId();
      request.url = url;
      request.bounds = guest->bounds;
      request.partition = guest->partition;
      request.allow_downloads = guest->allow_downloads;
      request.permissions = guest->permissions;
      request.popup_policy = guest->popup_policy;
      request.opened_by_popup = true;
      Create(std::move(request), [](bool success, const std::string& result) {
        if (!success) {
          std::cerr << "guest popup failed: " << result << std::endl;
        }
      });
      return;
    }
    case GuestPopupPolicy::kDeny:
    default:
      Emit(*guest, "guest.newWindow",
           GuestNewWindowJson(guest->id, url, disposition));
      return;
  }
}
