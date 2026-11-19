#include "common/accept_languages.h"

#if defined(OS_LINUX) || defined(OS_WIN)

#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <vector>

#if defined(OS_WIN)
#include <windows.h>
#endif

namespace {

#if defined(OS_WIN)
std::vector<std::string> SystemLanguageTags() {
  ULONG count = 0;
  ULONG size = 0;
  if (!GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &count, nullptr, &size)) {
    return {};
  }
  std::wstring names(size, L'\0');
  if (!GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &count, names.data(),
                                   &size)) {
    return {};
  }
  std::vector<std::string> tags;
  std::string tag;
  for (const wchar_t character : names) {
    if (character != L'\0') {
      tag.push_back(static_cast<char>(character));
    } else if (!tag.empty()) {
      tags.push_back(tag);
      tag.clear();
    }
  }
  return tags;
}
#else
// A locale name such as `de_DE.UTF-8@euro` as a language tag, `de-DE`.
std::string LanguageTag(std::string locale) {
  locale = locale.substr(0, locale.find_first_of(".@"));
  std::replace(locale.begin(), locale.end(), '_', '-');
  return locale;
}

std::vector<std::string> SystemLanguageTags() {
  std::string locale;
  for (const char* name : {"LC_ALL", "LC_MESSAGES", "LANG"}) {
    const char* value = std::getenv(name);
    if (value && *value) {
      locale = value;
      break;
    }
  }
  if (locale.empty() || locale == "C" || locale == "POSIX") {
    return {};
  }
  std::vector<std::string> tags;
  const char* language = std::getenv("LANGUAGE");
  const std::string preferred = language ? language : "";
  for (size_t start = 0; start < preferred.size();) {
    const size_t end = std::min(preferred.find(':', start), preferred.size());
    tags.push_back(LanguageTag(preferred.substr(start, end - start)));
    start = end + 1;
  }
  tags.push_back(LanguageTag(locale));
  return tags;
}
#endif

bool IsLanguage(const std::string& subtag) {
  return subtag.size() >= 2 && subtag.size() <= 3 &&
         std::all_of(subtag.begin(), subtag.end(), [](char character) {
           return std::isalpha(static_cast<unsigned char>(character));
         });
}

}  // namespace

std::string SystemAcceptLanguages() {
  std::vector<std::string> languages;
  const auto add = [&languages](const std::string& language) {
    if (std::find(languages.begin(), languages.end(), language) ==
        languages.end()) {
      languages.push_back(language);
    }
  };
  for (const std::string& tag : SystemLanguageTags()) {
    const std::string language = tag.substr(0, tag.find('-'));
    if (IsLanguage(language)) {
      add(tag);
      add(language);
    }
  }
  add("en-US");
  add("en");
  std::string list;
  for (const std::string& language : languages) {
    list += (list.empty() ? "" : ",") + language;
  }
  return list;
}

#endif
