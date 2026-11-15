#include "osr/accessibility/page_accessibility.h"

#include "include/wrapper/cef_helpers.h"
#include "osr/accessibility/tree_wire.h"
#include "osr/transport/message_kinds.h"

void PageAccessibility::Enable(CefRefPtr<CefBrowser> browser, bool enabled) {
  CEF_REQUIRE_UI_THREAD();
  enabled_ = enabled;
  browser->GetHost()->SetAccessibilityState(enabled ? STATE_ENABLED
                                                    : STATE_DISABLED);
}

void PageAccessibility::OnAccessibilityTreeChange(CefRefPtr<CefValue> value) {
  if (!enabled_ || value->GetType() != VTYPE_DICTIONARY) {
    return;
  }
  const std::vector<char> payload =
      sabine_osr::EncodeAccessibilityTree(value->GetDictionary());
  transport_.Send(kAccessibilityTree, 0, 0, 0, 0, payload.data(),
                  static_cast<uint32_t>(payload.size()));
}

void PageAccessibility::OnAccessibilityLocationChange(
    CefRefPtr<CefValue> value) {
  if (!enabled_ || value->GetType() != VTYPE_LIST) {
    return;
  }
  const std::vector<char> payload =
      sabine_osr::EncodeAccessibilityLocations(value->GetList());
  transport_.Send(kAccessibilityLocations, 0, 0, 0, 0, payload.data(),
                  static_cast<uint32_t>(payload.size()));
}
