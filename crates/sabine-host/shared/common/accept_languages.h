#ifndef SABINE_CEF_HOST_COMMON_ACCEPT_LANGUAGES_H_
#define SABINE_CEF_HOST_COMMON_ACCEPT_LANGUAGES_H_

#include <string>

#include "include/base/cef_build.h"

#if defined(OS_LINUX) || defined(OS_WIN)
// The languages pages hear the person reads, from `navigator.languages` and in
// requests, listed as Chromium lists them for a system language: each of the
// system's languages, followed by the language without its region, and English
// last. Every locale pack of a Sabine runtime holds the English text, from
// which Chromium would otherwise list English alone.
std::string SystemAcceptLanguages();
#endif

#endif
