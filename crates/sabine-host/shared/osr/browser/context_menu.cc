#include "osr/handler.h"

#include <cstdlib>
#include <string>
#include <vector>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"

using namespace sabine_osr;

namespace {

bool IsEditCommand(int command_id) {
  return (command_id >= MENU_ID_UNDO && command_id <= MENU_ID_SELECT_ALL) ||
         (command_id >= MENU_ID_SPELLCHECK_SUGGESTION_0 &&
          command_id <= MENU_ID_ADD_TO_DICTIONARY);
}

void KeepEditCommands(CefRefPtr<CefMenuModel> model) {
  for (size_t index = model->GetCount(); index-- > 0;) {
    const cef_menu_item_type_t type = model->GetTypeAt(index);
    if (type != MENUITEMTYPE_SEPARATOR &&
        (type == MENUITEMTYPE_SUBMENU ||
         !IsEditCommand(model->GetCommandIdAt(index)))) {
      model->RemoveAt(index);
    }
  }
  bool after_separator = true;
  for (size_t index = 0; index < model->GetCount();) {
    const bool separator = model->GetTypeAt(index) == MENUITEMTYPE_SEPARATOR;
    if (separator && after_separator) {
      model->RemoveAt(index);
      continue;
    }
    after_separator = separator;
    ++index;
  }
  const size_t count = model->GetCount();
  if (count > 0 && model->GetTypeAt(count - 1) == MENUITEMTYPE_SEPARATOR) {
    model->RemoveAt(count - 1);
  }
}

std::string MenuLabel(const std::string& label) {
  std::string text;
  text.reserve(label.size());
  for (size_t index = 0; index < label.size(); ++index) {
    if (label[index] == '&' && index + 1 < label.size()) {
      ++index;
    }
    text.push_back(label[index]);
  }
  return text;
}

std::string MenuItemsJson(CefRefPtr<CefMenuModel> model) {
  std::string items = "[";
  for (size_t index = 0; index < model->GetCount(); ++index) {
    if (index > 0) {
      items += ",";
    }
    if (model->GetTypeAt(index) == MENUITEMTYPE_SEPARATOR) {
      items += "{\"separator\":true}";
      continue;
    }
    items += "{\"id\":" + std::to_string(model->GetCommandIdAt(index)) +
             ",\"label\":" +
             JsString(MenuLabel(model->GetLabelAt(index).ToString())) +
             ",\"enabled\":" + (model->IsEnabledAt(index) ? "true" : "false") +
             "}";
  }
  return items + "]";
}

}  // namespace

void SabineOsrHandler::OnBeforeContextMenu(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefRefPtr<CefContextMenuParams> params,
    CefRefPtr<CefMenuModel> model) {
  CEF_REQUIRE_UI_THREAD();
  KeepEditCommands(model);
  if (dev_mode_) {
    if (model->GetCount() > 0) {
      model->AddSeparator();
    }
    model->AddItem(kInspectElementCommand, "Inspect element");
  }
}

bool SabineOsrHandler::RunContextMenu(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefRefPtr<CefContextMenuParams> params,
    CefRefPtr<CefMenuModel> model,
    CefRefPtr<CefRunContextMenuCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  int x = params->GetXCoord();
  int y = params->GetYCoord();
  if (const GuestView* guest = GuestForBrowser(browser)) {
    x += guest->bounds.x;
    y += guest->bounds.y;
  }
  const std::string items = MenuItemsJson(model);
  if (model->GetCount() == 0 ||
      !SendMessage(kContextMenu, 0, 0, x, y, items.data(),
                   static_cast<uint32_t>(items.size()))) {
    callback->Cancel();
    return true;
  }
  if (context_menu_callback_) {
    context_menu_callback_->Cancel();
  }
  context_menu_callback_ = callback;
  return true;
}

void SabineOsrHandler::OnContextMenuDismissed(CefRefPtr<CefBrowser> browser,
                                              CefRefPtr<CefFrame> frame) {
  CEF_REQUIRE_UI_THREAD();
  if (context_menu_callback_) {
    context_menu_callback_ = nullptr;
    SendMessage(kContextMenuDismissed, 0, 0, 0, 0, nullptr, 0);
  }
}

bool SabineOsrHandler::OnContextMenuCommand(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefRefPtr<CefContextMenuParams> params,
    int command_id,
    EventFlags event_flags) {
  CEF_REQUIRE_UI_THREAD();
  if (!dev_mode_ || command_id != kInspectElementCommand) {
    return false;
  }
  CefWindowInfo window_info;
  CefBrowserSettings settings;
  browser->GetHost()->ShowDevTools(
      window_info, nullptr, settings,
      CefPoint(params->GetXCoord(), params->GetYCoord()));
  return true;
}

void SabineOsrHandler::ChooseContextMenuCommand(int command_id) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefRunContextMenuCallback> callback = context_menu_callback_;
  context_menu_callback_ = nullptr;
  if (!callback) {
    return;
  }
  if (command_id < 0) {
    callback->Cancel();
  } else {
    callback->Continue(command_id, EVENTFLAG_NONE);
  }
}
