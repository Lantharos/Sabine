#include "osr/guest/registry.h"

#include <algorithm>

const char kSabinePopupGuestId[] = "__sabine_popup";
const char kSabineDevToolsGuestId[] = "__sabine_devtools";

GuestView* GuestRegistry::Find(const std::string& id) {
  if (id.empty()) {
    return nullptr;
  }
  const auto entry = guests_.find(id);
  return entry == guests_.end() ? nullptr : &entry->second;
}

const GuestView* GuestRegistry::Find(const std::string& id) const {
  if (id.empty()) {
    return nullptr;
  }
  const auto entry = guests_.find(id);
  return entry == guests_.end() ? nullptr : &entry->second;
}

GuestView* GuestRegistry::FindByBrowser(const CefRefPtr<CefBrowser>& browser) {
  if (!browser) {
    return nullptr;
  }
  for (auto& entry : guests_) {
    if (entry.second.browser && entry.second.browser->IsSame(browser)) {
      return &entry.second;
    }
  }
  return nullptr;
}

GuestView* GuestRegistry::Insert(GuestView guest) {
  const std::string id = guest.id;
  guests_[id] = std::move(guest);
  if (std::find(z_order_.begin(), z_order_.end(), id) == z_order_.end()) {
    z_order_.push_back(id);
  }
  return &guests_[id];
}

void GuestRegistry::Erase(const std::string& id) {
  guests_.erase(id);
  z_order_.erase(std::remove(z_order_.begin(), z_order_.end(), id),
                 z_order_.end());
}

void GuestRegistry::Raise(const std::string& id) {
  if (guests_.find(id) == guests_.end()) {
    return;
  }
  z_order_.erase(std::remove(z_order_.begin(), z_order_.end(), id),
                 z_order_.end());
  z_order_.push_back(id);
}

GuestView* GuestRegistry::TopmostAt(int x, int y) {
  if (covered_) {
    return nullptr;
  }
  for (auto id = z_order_.rbegin(); id != z_order_.rend(); ++id) {
    GuestView* guest = Find(*id);
    if (!guest || !guest->visible) {
      continue;
    }
    const CefRect& bounds = guest->bounds;
    if (x >= bounds.x && y >= bounds.y && x < bounds.x + bounds.width &&
        y < bounds.y + bounds.height) {
      return guest;
    }
  }
  return nullptr;
}

std::vector<GuestView*> GuestRegistry::InZOrder() {
  std::vector<GuestView*> ordered;
  ordered.reserve(z_order_.size());
  for (const auto& id : z_order_) {
    if (GuestView* guest = Find(id)) {
      ordered.push_back(guest);
    }
  }
  return ordered;
}

std::vector<const GuestView*> GuestRegistry::InZOrder() const {
  std::vector<const GuestView*> ordered;
  ordered.reserve(z_order_.size());
  for (const auto& id : z_order_) {
    if (const GuestView* guest = Find(id)) {
      ordered.push_back(guest);
    }
  }
  return ordered;
}
