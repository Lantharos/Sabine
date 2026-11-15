#include "osr/guest/guests.h"

#include <string>
#include <vector>

#include "include/wrapper/cef_helpers.h"
#include "osr/guest/events.h"
#include "osr/handler.h"

GuestView* Guests::ForBrowser(const CefRefPtr<CefBrowser>& browser) {
  if (GuestView* guest = registry_.FindByBrowser(browser)) {
    return guest;
  }
  if (owner_->browser_ && owner_->browser_->IsSame(browser)) {
    return nullptr;
  }
  return AwaitingBrowser();
}

GuestView* Guests::AwaitingBrowser() {
  GuestView* guest = registry_.Find(creations_.creating_id);
  return guest && !guest->browser ? guest : nullptr;
}

void Guests::Destroy(const std::string& id) {
  CEF_REQUIRE_UI_THREAD();
  const bool canceled = CancelPending(id);
  GuestView* guest = registry_.Find(id);
  if (!guest) {
    if (canceled && !IsInternalGuestId(id)) {
      owner_->EmitPrimaryEvent("guest.destroyed", GuestIdJson(id));
    }
    return;
  }
  const GuestView removed = *guest;
  registry_.Erase(id);
  if (id == kSabineDevToolsGuestId) {
    owner_->devtools_inspected_ = nullptr;
    owner_->devtools_registration_ = nullptr;
  }
  const bool restore_primary_focus = focused_id_ == id;
  if (focused_id_ == id) {
    focused_id_.clear();
  }
  owner_->downloads_->CancelGuest(id);
  SendHidden(removed);
  if (removed.browser) {
    removed.browser->GetHost()->CloseBrowser(true);
  }
  if (restore_primary_focus && owner_->browser_) {
    owner_->browser_->GetHost()->SetFocus(true);
    owner_->SendFocusedImeState();
  }
  Emit(removed, "guest.destroyed", GuestIdJson(id));
  if (id == kSabinePopupGuestId) {
    owner_->EmitBridgeEvent("\"popup.close\"", "{}");
  }
}

void Guests::DestroyAll() {
  std::vector<std::string> ids;
  for (const GuestView* view : registry_.InZOrder()) {
    ids.push_back(view->id);
  }
  for (const auto& id : ids) {
    Destroy(id);
  }
}

void Guests::Focus(const std::string& id) {
  CEF_REQUIRE_UI_THREAD();
  if (!id.empty() && !registry_.Find(id)) {
    return;
  }
  if (focused_id_ != id) {
    if (GuestView* previous = registry_.Find(focused_id_)) {
      if (previous->browser) {
        previous->browser->GetHost()->SetFocus(false);
      }
    }
  }
  focused_id_ = id;
  if (id.empty()) {
    if (owner_->browser_) {
      owner_->browser_->GetHost()->SetFocus(true);
    }
    owner_->SendFocusedImeState();
    return;
  }
  registry_.Raise(id);
  SendOrder();
  if (owner_->browser_) {
    owner_->browser_->GetHost()->SetFocus(false);
  }
  GuestView* guest = registry_.Find(id);
  if (guest && guest->browser) {
    guest->browser->GetHost()->SetFocus(true);
  }
  owner_->SendFocusedImeState();
}

void Guests::DismissPopup() {
  if (registry_.Find(kSabinePopupGuestId)) {
    Destroy(kSabinePopupGuestId);
  }
}

void Guests::ApplyBounds(GuestView& guest) {
  if (!guest.browser) {
    return;
  }
  CefRefPtr<CefBrowserHost> host = guest.browser->GetHost();
  host->NotifyScreenInfoChanged();
  host->WasResized();
  host->Invalidate(PET_VIEW);
}

void Guests::ApplyVisibility(GuestView& guest) {
  if (!guest.browser) {
    return;
  }
  CefRefPtr<CefBrowserHost> host = guest.browser->GetHost();
  const bool hidden =
      !guest.visible || owner_->view_hidden_ || registry_.Covered();
  host->WasHidden(hidden);
  if (!hidden) {
    host->Invalidate(PET_VIEW);
    return;
  }
  if (!guest.visible) {
    SendHidden(guest);
  }
  if (!guest.visible && focused_id_ == guest.id) {
    host->SetFocus(false);
    focused_id_.clear();
    if (owner_->browser_) {
      owner_->browser_->GetHost()->SetFocus(true);
    }
  }
}

void Guests::ApplyLifecycle() {
  CEF_REQUIRE_UI_THREAD();
  const int frame_rate = owner_->FrameRate();
  for (GuestView* guest : registry_.InZOrder()) {
    if (!guest->browser) {
      continue;
    }
    guest->browser->GetHost()->SetWindowlessFrameRate(frame_rate);
    ApplyVisibility(*guest);
  }
}

void Guests::NotifyScreenInfo() {
  CEF_REQUIRE_UI_THREAD();
  if (GuestView* devtools = registry_.Find(kSabineDevToolsGuestId)) {
    devtools->bounds = owner_->DevToolsBounds();
  }
  for (GuestView* guest : registry_.InZOrder()) {
    ApplyBounds(*guest);
  }
}

void Guests::FollowPageZoom(double zoom) {
  for (GuestView* guest : registry_.InZOrder()) {
    if (guest->FollowsPageZoom()) {
      guest->page_zoom = zoom;
      ApplyBounds(*guest);
    }
  }
}

void Guests::Repaint() {
  for (GuestView* guest : registry_.InZOrder()) {
    if (guest->browser && guest->visible) {
      guest->browser->GetHost()->Invalidate(PET_VIEW);
      guest->browser->GetHost()->Invalidate(PET_POPUP);
    }
  }
}

void Guests::Emit(const GuestView& guest,
                  const std::string& name,
                  const std::string& payload) {
  if (guest.ShownToPage()) {
    owner_->EmitPrimaryEvent(name, payload);
  }
}

void Guests::MarkPainted(GuestView& guest) {
  if (guest.painted) {
    return;
  }
  guest.painted = true;
  if (guest.id == kSabinePopupGuestId) {
    owner_->EmitBridgeEvent("\"popup.open\"", "{}");
  }
}

bool Guests::SendPaint(const GuestView& guest,
                       const void* buffer,
                       int width,
                       int height,
                       const CefRenderHandler::RectList& dirty_rects) {
  const CefRect view = guest.ViewBounds();
  return owner_->software_paint_.Send(
      {guest.browser->GetIdentifier(), false}, PaintSurface::kGuest, guest.id,
      view.x, view.y, buffer, width, height, dirty_rects);
}

void Guests::SendHidden(const GuestView& guest) {
  transport_.Send(kGuestHidden, 0, 0, 0, 0, guest.id.data(),
                  static_cast<uint32_t>(guest.id.size()));
  if (guest.id == kSabinePopupGuestId) {
    transport_.Send(kPopupHidden, 0, 0, 0, 0, nullptr, 0);
  }
}

void Guests::HideCovered() {
  if (!pending_cover_ || !registry_.Covered()) {
    return;
  }
  pending_cover_ = false;
  for (GuestView* guest : registry_.InZOrder()) {
    if (guest->visible) {
      SendHidden(*guest);
    }
  }
}

void Guests::HidePopup(const GuestView& guest) {
  SendPopupHidden(guest);
  popup_rect_ = CefRect();
}

void Guests::ResizePopup(const GuestView& guest, const CefRect& rect) {
  if (popup_rect_.x != rect.x || popup_rect_.y != rect.y ||
      popup_rect_.width != rect.width || popup_rect_.height != rect.height) {
    SendPopupHidden(guest);
  }
  popup_rect_ = rect;
}

void Guests::SendPopupHidden(const GuestView& guest) {
  const std::string popup_id = guest.id + "/popup";
  transport_.Send(kGuestHidden, 0, 0, 0, 0, popup_id.data(),
                  static_cast<uint32_t>(popup_id.size()));
}

void Guests::SendOrder() {
  std::vector<char> payload;
  for (const GuestView* guest : registry_.InZOrder()) {
    payload.insert(payload.end(), guest->id.begin(), guest->id.end());
    payload.push_back('\0');
  }
  transport_.Send(kGuestOrder, 0, 0, 0, 0, payload.data(),
                  static_cast<uint32_t>(payload.size()));
}
