#include "osr/dialogs/dialogs.h"

#include <string>
#include <vector>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/handler.h"

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
    if (type == MENUITEMTYPE_SEPARATOR) {
      continue;
    }
    if (type == MENUITEMTYPE_SUBMENU) {
      CefRefPtr<CefMenuModel> submenu = model->GetSubMenuAt(index);
      if (submenu) {
        KeepEditCommands(submenu);
      }
      if (!submenu || submenu->GetCount() == 0) {
        model->RemoveAt(index);
      }
    } else if (!IsEditCommand(model->GetCommandIdAt(index))) {
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

constexpr int kOpenDevToolsCommand = MENU_ID_USER_FIRST;
constexpr int kCloseDevToolsCommand = MENU_ID_USER_FIRST + 1;

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
    const cef_menu_item_type_t type = model->GetTypeAt(index);
    if (type == MENUITEMTYPE_SEPARATOR) {
      items += "{\"separator\":true}";
      continue;
    }
    items += "{\"id\":" + std::to_string(model->GetCommandIdAt(index)) +
             ",\"label\":" +
             JsString(MenuLabel(model->GetLabelAt(index).ToString())) +
             ",\"enabled\":" + (model->IsEnabledAt(index) ? "true" : "false");
    if (type == MENUITEMTYPE_CHECK || type == MENUITEMTYPE_RADIO) {
      items += std::string(",\"mark\":") +
               (type == MENUITEMTYPE_CHECK ? "\"check\"" : "\"radio\"") +
               ",\"checked\":" + (model->IsCheckedAt(index) ? "true" : "false");
    }
    if (type == MENUITEMTYPE_SUBMENU) {
      items += ",\"items\":" + MenuItemsJson(model->GetSubMenuAt(index));
    }
    items += "}";
  }
  return items + "]";
}

}  // namespace

void PageDialogs::OnBeforeContextMenu(CefRefPtr<CefBrowser> browser,
                                      CefRefPtr<CefFrame> frame,
                                      CefRefPtr<CefContextMenuParams> params,
                                      CefRefPtr<CefMenuModel> model) {
  CEF_REQUIRE_UI_THREAD();
  KeepEditCommands(model);
  if (!owner_->dev_mode_) {
    return;
  }
  if (model->GetCount() > 0) {
    model->AddSeparator();
  }
  if (owner_->devtools_inspected_) {
    model->AddItem(kCloseDevToolsCommand, "Close DevTools");
  } else {
    model->AddItem(kOpenDevToolsCommand, "Open DevTools");
  }
}

bool PageDialogs::RunContextMenu(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefRefPtr<CefContextMenuParams> params,
    CefRefPtr<CefMenuModel> model,
    CefRefPtr<CefRunContextMenuCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  CefRect point(params->GetXCoord(), params->GetYCoord(), 0, 0);
  if (const GuestView* guest = owner_->GuestForBrowser(browser)) {
    point = guest->ToView(point);
  }
  const std::string items = MenuItemsJson(model);
  if (model->GetCount() == 0 ||
      !owner_->SendMessage(kContextMenu, 0, 0, point.x, point.y, items.data(),
                           static_cast<uint32_t>(items.size()))) {
    callback->Cancel();
    return true;
  }
  if (context_menu_) {
    context_menu_->Cancel();
  }
  context_menu_ = callback;
  return true;
}

void PageDialogs::OnContextMenuDismissed(CefRefPtr<CefBrowser> browser,
                                         CefRefPtr<CefFrame> frame) {
  CEF_REQUIRE_UI_THREAD();
  if (context_menu_) {
    context_menu_ = nullptr;
    owner_->SendMessage(kContextMenuDismissed, 0, 0, 0, 0, nullptr, 0);
  }
}

bool PageDialogs::OnContextMenuCommand(CefRefPtr<CefBrowser> browser,
                                       CefRefPtr<CefFrame> frame,
                                       CefRefPtr<CefContextMenuParams> params,
                                       int command_id,
                                       EventFlags event_flags) {
  CEF_REQUIRE_UI_THREAD();
  if (!owner_->dev_mode_) {
    return false;
  }
  if (command_id == kOpenDevToolsCommand) {
    owner_->ShowDevTools(browser);
    return true;
  }
  if (command_id == kCloseDevToolsCommand) {
    owner_->CloseDevTools();
    return true;
  }
  return false;
}

void PageDialogs::ChooseContextMenuCommand(int command_id) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefRunContextMenuCallback> callback = context_menu_;
  context_menu_ = nullptr;
  if (!callback) {
    return;
  }
  if (command_id < 0) {
    callback->Cancel();
  } else {
    callback->Continue(command_id, EVENTFLAG_NONE);
  }
}
