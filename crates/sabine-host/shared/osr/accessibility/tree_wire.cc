#include "osr/accessibility/tree_wire.h"

#include <array>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <string>
#include <string_view>
#include <unordered_map>

namespace sabine_osr {

namespace {

enum ValueType : uint8_t {
  kString = 1,
  kInt = 2,
  kDouble = 3,
  kBool = 4,
  kIntList = 5,
};

constexpr int kNoNode = -1;

const std::unordered_map<std::string_view, uint8_t>& AttributeCodes() {
  static const std::unordered_map<std::string_view, uint8_t> codes = {
      {"name", 1},
      {"value", 2},
      {"description", 3},
      {"placeholder", 4},
      {"url", 5},
      {"childTreeId", 6},
      {"liveStatus", 7},
      {"roleDescription", 9},
      {"keyShortcuts", 10},
      {"accessKey", 11},
      {"tooltip", 12},
      {"language", 13},
      {"checkedState", 14},
      {"invalidState", 15},
      {"restriction", 16},
      {"defaultActionVerb", 17},
      {"ariaCurrentState", 18},
      {"sortDirection", 19},
      {"textDirection", 20},
      {"inputType", 21},
      {"ariaValueText", 22},
      {"scrollX", 30},
      {"scrollXMin", 31},
      {"scrollXMax", 32},
      {"scrollY", 33},
      {"scrollYMin", 34},
      {"scrollYMax", 35},
      {"textSelStart", 36},
      {"textSelEnd", 37},
      {"hierarchicalLevel", 38},
      {"setSize", 39},
      {"posInSet", 40},
      {"activedescendantId", 41},
      {"tableRowCount", 42},
      {"tableColumnCount", 43},
      {"tableRowIndex", 44},
      {"tableColumnIndex", 45},
      {"tableCellRowIndex", 46},
      {"tableCellColumnIndex", 47},
      {"tableCellRowSpan", 48},
      {"tableCellColumnSpan", 49},
      {"haspopup", 50},
      {"valueForRange", 60},
      {"minValueForRange", 61},
      {"maxValueForRange", 62},
      {"stepValueForRange", 63},
      {"busy", 70},
      {"liveAtomic", 71},
      {"modal", 72},
      {"selected", 73},
      {"scrollable", 74},
      {"nonAtomicTextFieldRoot", 75},
      {"characterOffsets", 80},
      {"wordStarts", 81},
      {"labelledbyIds", 83},
      {"describedbyIds", 84},
      {"controlsIds", 85},
      {"detailsIds", 86},
      {"flowtoIds", 87},
      {"errorMessageIds", 88},
  };
  return codes;
}

const std::unordered_map<std::string_view, uint32_t>& StateBits() {
  static const std::unordered_map<std::string_view, uint32_t> bits = {
      {"collapsed", 0},  {"default", 1},    {"editable", 2},
      {"expanded", 3},   {"focusable", 4},  {"horizontal", 5},
      {"hovered", 6},    {"ignored", 7},    {"invisible", 8},
      {"linked", 9},     {"multiline", 10}, {"multiselectable", 11},
      {"protected", 12}, {"required", 13},  {"richlyEditable", 14},
      {"vertical", 15},  {"visited", 16},   {"autofillAvailable", 17},
  };
  return bits;
}

const std::unordered_map<std::string_view, uint32_t>& ActionBits() {
  static const std::unordered_map<std::string_view, uint32_t> bits = {
      {"collapse", 0},       {"decrement", 1},
      {"doDefault", 2},      {"expand", 3},
      {"focus", 4},          {"increment", 5},
      {"scrollBackward", 6}, {"scrollForward", 7},
      {"scrollUp", 8},       {"scrollDown", 9},
      {"scrollLeft", 10},    {"scrollRight", 11},
      {"setValue", 12},      {"showContextMenu", 13},
      {"setSelection", 14},
  };
  return bits;
}

class Writer {
 public:
  void U8(uint8_t value) { bytes_.push_back(static_cast<char>(value)); }
  void U32(uint32_t value) {
    for (int shift = 0; shift < 32; shift += 8) {
      U8(static_cast<uint8_t>((value >> shift) & 0xff));
    }
  }
  void I32(int32_t value) { U32(static_cast<uint32_t>(value)); }
  void F32(float value) {
    uint32_t bits;
    std::memcpy(&bits, &value, sizeof(bits));
    U32(bits);
  }
  void Str(const std::string& value) {
    U32(static_cast<uint32_t>(value.size()));
    bytes_.insert(bytes_.end(), value.begin(), value.end());
  }
  size_t Reserve32() {
    const size_t at = bytes_.size();
    U32(0);
    return at;
  }
  void Patch32(size_t at, uint32_t value) {
    for (int shift = 0; shift < 32; shift += 8) {
      bytes_[at + shift / 8] = static_cast<char>((value >> shift) & 0xff);
    }
  }
  std::vector<char> Take() { return std::move(bytes_); }

 private:
  std::vector<char> bytes_;
};

std::string String(CefRefPtr<CefDictionaryValue> dict, const char* key) {
  return dict->GetType(key) == VTYPE_STRING ? dict->GetString(key).ToString()
                                            : std::string();
}

int Int(CefRefPtr<CefDictionaryValue> dict, const char* key, int fallback) {
  return dict->GetType(key) == VTYPE_INT ? dict->GetInt(key) : fallback;
}

uint32_t Bits(CefRefPtr<CefListValue> names,
              const std::unordered_map<std::string_view, uint32_t>& table) {
  uint32_t bits = 0;
  for (size_t i = 0; names && i < names->GetSize(); ++i) {
    const auto bit = table.find(names->GetString(i).ToString());
    if (bit != table.end()) {
      bits |= 1u << bit->second;
    }
  }
  return bits;
}

// Reads the row-major 4x4 matrix gfx::Transform::ToString writes and keeps
// its 2D part, as a, b, c, d, e, f of x' = a x + c y + e, y' = b x + d y + f.
bool ParseTransform(const std::string& text, std::array<float, 6>* affine) {
  std::array<double, 16> matrix{};
  const char* cursor = text.c_str();
  for (double& entry : matrix) {
    while (*cursor && !(*cursor == '-' || *cursor == '+' || *cursor == '.' ||
                        (*cursor >= '0' && *cursor <= '9'))) {
      ++cursor;
    }
    char* end = nullptr;
    entry = std::strtod(cursor, &end);
    if (end == cursor) {
      return false;
    }
    cursor = end;
  }
  *affine = {static_cast<float>(matrix[0]), static_cast<float>(matrix[4]),
             static_cast<float>(matrix[1]), static_cast<float>(matrix[5]),
             static_cast<float>(matrix[3]), static_cast<float>(matrix[7])};
  return true;
}

void WriteLocation(Writer& out,
                   int container,
                   CefRefPtr<CefDictionaryValue> bounds,
                   const std::string& transform) {
  out.I32(container);
  for (const char* key : {"x", "y", "width", "height"}) {
    out.F32(bounds && bounds->GetType(key) == VTYPE_DOUBLE
                ? static_cast<float>(bounds->GetDouble(key))
                : 0.0f);
  }
  std::array<float, 6> affine;
  if (!transform.empty() && ParseTransform(transform, &affine)) {
    out.U8(1);
    for (float coefficient : affine) {
      out.F32(coefficient);
    }
  } else {
    out.U8(0);
  }
}

void WriteIntList(Writer& out, CefRefPtr<CefListValue> list) {
  out.U32(static_cast<uint32_t>(list->GetSize()));
  for (size_t i = 0; i < list->GetSize(); ++i) {
    out.I32(list->GetType(i) == VTYPE_INT ? list->GetInt(i) : 0);
  }
}

void WriteAttributes(Writer& out, CefRefPtr<CefDictionaryValue> attributes) {
  const size_t count_at = out.Reserve32();
  uint32_t count = 0;
  CefDictionaryValue::KeyList keys;
  if (attributes) {
    attributes->GetKeys(keys);
  }
  const auto& codes = AttributeCodes();
  for (const CefString& key : keys) {
    const auto code = codes.find(key.ToString());
    if (code == codes.end()) {
      continue;
    }
    switch (attributes->GetType(key)) {
      case VTYPE_STRING:
        out.U8(code->second);
        out.U8(kString);
        out.Str(attributes->GetString(key));
        break;
      case VTYPE_INT:
        out.U8(code->second);
        out.U8(kInt);
        out.I32(attributes->GetInt(key));
        break;
      case VTYPE_DOUBLE:
        out.U8(code->second);
        out.U8(kDouble);
        out.F32(static_cast<float>(attributes->GetDouble(key)));
        break;
      case VTYPE_BOOL:
        out.U8(code->second);
        out.U8(kBool);
        out.U8(attributes->GetBool(key) ? 1 : 0);
        break;
      case VTYPE_LIST:
        out.U8(code->second);
        out.U8(kIntList);
        WriteIntList(out, attributes->GetList(key));
        break;
      default:
        continue;
    }
    ++count;
  }
  out.Patch32(count_at, count);
}

void WriteNode(Writer& out, CefRefPtr<CefDictionaryValue> node) {
  out.I32(Int(node, "id", 0));
  out.Str(String(node, "role"));
  out.U32(Bits(node->GetList("state"), StateBits()));
  out.U32(Bits(node->GetList("actions"), ActionBits()));
  WriteLocation(out, Int(node, "offset_container_id", kNoNode),
                node->GetDictionary("location"), String(node, "transform"));
  CefRefPtr<CefListValue> children = node->GetList("child_ids");
  if (children) {
    WriteIntList(out, children);
  } else {
    out.U32(0);
  }
  WriteAttributes(out, node->GetDictionary("attributes"));
}

void WriteTreeData(Writer& out, CefRefPtr<CefDictionaryValue> data) {
  out.Str(String(data, "parent_tree_id"));
  out.Str(String(data, "focused_tree_id"));
  out.I32(Int(data, "focus_id", kNoNode));
  out.I32(Int(data, "sel_anchor_object_id", kNoNode));
  out.I32(Int(data, "sel_anchor_offset", 0));
  out.I32(Int(data, "sel_focus_object_id", kNoNode));
  out.I32(Int(data, "sel_focus_offset", 0));
}

void WriteUpdate(Writer& out, CefRefPtr<CefDictionaryValue> update) {
  CefRefPtr<CefDictionaryValue> data = update->GetDictionary("tree_data");
  out.U8(data ? 1 : 0);
  if (data) {
    WriteTreeData(out, data);
  }
  out.I32(Int(update, "node_id_to_clear", 0));
  out.I32(Int(update, "root_id", 0));
  CefRefPtr<CefListValue> nodes = update->GetList("nodes");
  const size_t count = nodes ? nodes->GetSize() : 0;
  out.U32(static_cast<uint32_t>(count));
  for (size_t i = 0; i < count; ++i) {
    WriteNode(out, nodes->GetDictionary(i));
  }
}

}  // namespace

std::vector<char> EncodeAccessibilityTree(CefRefPtr<CefDictionaryValue> event) {
  Writer out;
  out.Str(String(event, "ax_tree_id"));
  CefRefPtr<CefListValue> updates = event->GetList("updates");
  const size_t count = updates ? updates->GetSize() : 0;
  out.U32(static_cast<uint32_t>(count));
  for (size_t i = 0; i < count; ++i) {
    WriteUpdate(out, updates->GetDictionary(i));
  }
  return out.Take();
}

std::vector<char> EncodeAccessibilityLocations(
    CefRefPtr<CefListValue> changes) {
  Writer out;
  const size_t count = changes->GetSize();
  out.Str(count ? String(changes->GetDictionary(0), "ax_tree_id")
                : std::string());
  out.U32(static_cast<uint32_t>(count));
  for (size_t i = 0; i < count; ++i) {
    CefRefPtr<CefDictionaryValue> change = changes->GetDictionary(i);
    CefRefPtr<CefDictionaryValue> location =
        change->GetDictionary("new_location");
    out.I32(Int(change, "id", 0));
    WriteLocation(out, Int(location, "offset_container_id", kNoNode),
                  location->GetDictionary("bounds"),
                  String(location, "transform"));
  }
  return out.Take();
}

}  // namespace sabine_osr
