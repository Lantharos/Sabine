#include "osr/accessibility/actions.h"

#include <cstdlib>
#include <initializer_list>
#include <utility>

#include "common/json.h"
#include "include/wrapper/cef_helpers.h"
#include "osr/common/strings.h"

namespace {

constexpr char kClick[] =
    "function(){const e=this.nodeType===1?this:this.parentElement;"
    "e&&e.click()}";
constexpr char kSetValue[] =
    "function(value){const e=this.nodeType===1?this:this.parentElement;"
    "if(!e)return;e.focus();if(e.isContentEditable){e.textContent=value}"
    "else{const d=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(e),"
    "'value');d&&d.set?d.set.call(e,value):(e.value=value)}"
    "e.dispatchEvent(new InputEvent('input',{bubbles:true,"
    "inputType:'insertReplacementText',data:value}));"
    "e.dispatchEvent(new Event('change',{bubbles:true}))}";
constexpr char kSelectText[] =
    "function(start,end){this.focus();this.setSelectionRange(start,end)}";
constexpr char kSelectRange[] =
    "function(anchorOffset,focusOffset,focusNode){"
    "const e=(this.nodeType===1?this:this.parentElement)"
    ".closest('[contenteditable]');e&&e.focus();"
    "getSelection().setBaseAndExtent(this,anchorOffset,focusNode,"
    "focusOffset)}";
constexpr char kStep[] =
    "function(up){if(!this.stepUp){this.dispatchEvent(new KeyboardEvent("
    "'keydown',{key:up?'ArrowUp':'ArrowDown',bubbles:true}));return}"
    "up?this.stepUp():this.stepDown();"
    "this.dispatchEvent(new Event('input',{bubbles:true}));"
    "this.dispatchEvent(new Event('change',{bubbles:true}))}";
constexpr char kScroll[] =
    "function(x,y,page){const e=this.nodeType===9?this.scrollingElement:this;"
    "const w=page?e.clientWidth*0.875:40,h=page?e.clientHeight*0.875:40;"
    "e.scrollBy({left:x*w,top:y*h})}";

int NodeId(const std::string& value) {
  return std::atoi(value.c_str());
}

CefRefPtr<CefDictionaryValue> NodeParams(int node) {
  CefRefPtr<CefDictionaryValue> params = CefDictionaryValue::Create();
  params->SetInt("backendNodeId", node);
  return params;
}

CefRefPtr<CefDictionaryValue> Literal(CefRefPtr<CefValue> value) {
  CefRefPtr<CefDictionaryValue> argument = CefDictionaryValue::Create();
  argument->SetValue("value", value);
  return argument;
}

CefRefPtr<CefValue> IntValue(const std::string& text) {
  CefRefPtr<CefValue> value = CefValue::Create();
  value->SetInt(std::atoi(text.c_str()));
  return value;
}

CefRefPtr<CefValue> StringValue(const std::string& text) {
  CefRefPtr<CefValue> value = CefValue::Create();
  value->SetString(text);
  return value;
}

CefRefPtr<CefValue> BoolValue(bool flag) {
  CefRefPtr<CefValue> value = CefValue::Create();
  value->SetBool(flag);
  return value;
}

CefRefPtr<CefListValue> Arguments(
    std::initializer_list<CefRefPtr<CefValue>> values) {
  CefRefPtr<CefListValue> arguments = CefListValue::Create();
  for (const auto& value : values) {
    arguments->SetDictionary(arguments->GetSize(), Literal(value));
  }
  return arguments;
}

}  // namespace

void AccessibilityActions::Perform(CefRefPtr<CefBrowser> browser,
                                   const std::vector<std::string>& parts) {
  CEF_REQUIRE_UI_THREAD();
  if (parts.size() < 3) {
    return;
  }
  if (!registration_) {
    registration_ = browser->GetHost()->AddDevToolsMessageObserver(this);
  }
  const std::string& verb = parts[1];
  const int node = NodeId(parts[2]);
  if (verb == "focus") {
    Send(browser, "DOM.focus", NodeParams(node), nullptr);
  } else if (verb == "scroll_into_view") {
    Send(browser, "DOM.scrollIntoViewIfNeeded", NodeParams(node), nullptr);
  } else if (verb == "click") {
    Call(browser, {kClick, Arguments({}), {node}});
  } else if (verb == "set_value" && parts.size() == 4) {
    Call(
        browser,
        {kSetValue,
         Arguments({StringValue(sabine_osr::DecodeControlComponent(parts[3]))}),
         {node}});
  } else if (verb == "select_text" && parts.size() == 5) {
    Call(browser, {kSelectText,
                   Arguments({IntValue(parts[3]), IntValue(parts[4])}),
                   {node}});
  } else if (verb == "select_range" && parts.size() == 6) {
    Call(browser, {kSelectRange,
                   Arguments({IntValue(parts[3]), IntValue(parts[5])}),
                   {node, NodeId(parts[4])}});
  } else if (verb == "step" && parts.size() == 4) {
    Call(browser, {kStep, Arguments({BoolValue(parts[3] == "1")}), {node}});
  } else if (verb == "scroll" && parts.size() == 6) {
    Call(browser, {kScroll,
                   Arguments({IntValue(parts[3]), IntValue(parts[4]),
                              BoolValue(parts[5] == "1")}),
                   {node}});
  }
}

void AccessibilityActions::Send(CefRefPtr<CefBrowser> browser,
                                const std::string& method,
                                CefRefPtr<CefDictionaryValue> params,
                                ElementCall* call) {
  const int message_id = next_message_id_++;
  if (call) {
    pending_.emplace(message_id, std::move(*call));
  }
  browser->GetHost()->ExecuteDevToolsMethod(message_id, method, params);
}

void AccessibilityActions::Call(CefRefPtr<CefBrowser> browser,
                                ElementCall call) {
  call.object_group =
      "sabine-accessibility-" + std::to_string(next_object_group_++);
  Continue(browser, std::move(call));
}

void AccessibilityActions::Continue(CefRefPtr<CefBrowser> browser,
                                    ElementCall call) {
  if (call.objects.size() < call.nodes.size()) {
    CefRefPtr<CefDictionaryValue> params =
        NodeParams(call.nodes[call.objects.size()]);
    params->SetString("objectGroup", call.object_group);
    Send(browser, "DOM.resolveNode", params, &call);
    return;
  }
  CefRefPtr<CefListValue> arguments = call.arguments->Copy();
  for (size_t i = 1; i < call.objects.size(); ++i) {
    CefRefPtr<CefDictionaryValue> object = CefDictionaryValue::Create();
    object->SetString("objectId", call.objects[i]);
    arguments->SetDictionary(arguments->GetSize(), object);
  }
  CefRefPtr<CefDictionaryValue> params = CefDictionaryValue::Create();
  params->SetString("objectId", call.objects[0]);
  params->SetString("functionDeclaration", call.script);
  params->SetList("arguments", arguments);
  params->SetBool("userGesture", true);
  params->SetBool("silent", true);
  Send(browser, "Runtime.callFunctionOn", params, &call);
}

void AccessibilityActions::OnDevToolsMethodResult(CefRefPtr<CefBrowser> browser,
                                                  int message_id,
                                                  bool success,
                                                  const void* result,
                                                  size_t result_size) {
  const auto entry = pending_.find(message_id);
  if (entry == pending_.end()) {
    return;
  }
  ElementCall call = std::move(entry->second);
  pending_.erase(entry);
  const bool resolving = call.objects.size() < call.nodes.size();
  if (resolving && success) {
    const std::string object =
        JsonObject::Parse(
            std::string(static_cast<const char*>(result), result_size))
            .Object("object")
            .String("objectId");
    if (!object.empty()) {
      call.objects.push_back(object);
      Continue(browser, std::move(call));
      return;
    }
  }
  if (!call.objects.empty()) {
    CefRefPtr<CefDictionaryValue> params = CefDictionaryValue::Create();
    params->SetString("objectGroup", call.object_group);
    Send(browser, "Runtime.releaseObjectGroup", params, nullptr);
  }
}
