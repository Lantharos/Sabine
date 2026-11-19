#include "osr/dialogs/dialogs.h"

#include <string>
#include <vector>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/handler.h"

namespace {

constexpr char kContextEvent[] = "menu.context";

std::string Flag(const char* name, bool value) {
  return std::string(",\"") + name + "\":" + (value ? "true" : "false");
}

std::string OptionalString(const std::string& value) {
  return value.empty() ? "null" : JsString(value);
}

std::string Suggestions(CefRefPtr<CefContextMenuParams> params) {
  std::vector<CefString> suggestions;
  params->GetDictionarySuggestions(suggestions);
  std::string list = "[";
  for (const CefString& suggestion : suggestions) {
    list += (list.size() > 1 ? "," : "") + JsString(suggestion.ToString());
  }
  return list + "]";
}

}  // namespace

// Pages draw their own context menus, so Chromium's never opens. The app's
// page hears where it was asked for and what applies there, which a page
// can't tell from the DOM alone, as for the guests it shows.
void PageDialogs::OnBeforeContextMenu(CefRefPtr<CefBrowser> browser,
                                      CefRefPtr<CefFrame> frame,
                                      CefRefPtr<CefContextMenuParams> params,
                                      CefRefPtr<CefMenuModel> model) {
  CEF_REQUIRE_UI_THREAD();
  model->Clear();
  const GuestView* guest = owner_->guests_.FindByBrowser(browser);
  if (guest && !guest->ShownToPage()) {
    return;
  }
  context_browser_ = browser;
  CefRect point(params->GetXCoord(), params->GetYCoord(), 0, 0);
  if (guest) {
    point = guest->ToView(point);
  }
  const int edit = params->GetEditStateFlags();
  const std::string payload =
      "{\"x\":" + std::to_string(point.x / owner_->page_zoom_) +
      ",\"y\":" + std::to_string(point.y / owner_->page_zoom_) +
      ",\"guest\":" + OptionalString(guest ? guest->id : std::string()) +
      Flag("editable", params->IsEditable()) +
      ",\"selection\":" + JsString(params->GetSelectionText().ToString()) +
      ",\"link\":" + OptionalString(params->GetLinkUrl().ToString()) +
      ",\"misspelled\":" +
      OptionalString(params->GetMisspelledWord().ToString()) +
      ",\"suggestions\":" + Suggestions(params) +
      Flag("canUndo", edit & CM_EDITFLAG_CAN_UNDO) +
      Flag("canRedo", edit & CM_EDITFLAG_CAN_REDO) +
      Flag("canCut", edit & CM_EDITFLAG_CAN_CUT) +
      Flag("canCopy", edit & CM_EDITFLAG_CAN_COPY) +
      Flag("canPaste", edit & CM_EDITFLAG_CAN_PASTE) +
      Flag("canSelectAll", edit & CM_EDITFLAG_CAN_SELECT_ALL) + "}";
  owner_->EmitPrimaryEvent(kContextEvent, payload);
}

void PageDialogs::ReplaceMisspelling(const std::string& word) {
  CEF_REQUIRE_UI_THREAD();
  if (context_browser_) {
    context_browser_->GetHost()->ReplaceMisspelling(word);
  }
}

void PageDialogs::Forget(CefRefPtr<CefBrowser> browser) {
  if (context_browser_ && context_browser_->IsSame(browser)) {
    context_browser_ = nullptr;
  }
}
