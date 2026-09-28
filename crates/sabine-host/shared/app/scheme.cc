#include "app/scheme.h"

#include <algorithm>
#include <cstdio>
#include <filesystem>
#include <optional>
#include <system_error>
#include <utility>

#include "include/cef_parser.h"
#include "include/cef_resource_handler.h"

namespace sabine_app {
namespace {

namespace fs = std::filesystem;

constexpr char kScheme[] = "sabine";
constexpr char kAppHost[] = "app";
constexpr char kFileHost[] = "file";
constexpr char kAppOrigin[] = "sabine://app";

struct ByteRange {
  uint64_t start = 0;
  uint64_t length = 0;
  bool partial = false;
};

FILE* OpenFile(const fs::path& path, uint64_t offset) {
#if defined(OS_WIN)
  FILE* file = _wfopen(path.c_str(), L"rb");
  if (file && _fseeki64(file, static_cast<int64_t>(offset), SEEK_SET) != 0) {
#else
  FILE* file = std::fopen(path.c_str(), "rb");
  if (file && fseeko(file, static_cast<off_t>(offset), SEEK_SET) != 0) {
#endif
    std::fclose(file);
    return nullptr;
  }
  return file;
}

bool Contains(const fs::path& root, const fs::path& path) {
  return std::mismatch(root.begin(), root.end(), path.begin(), path.end())
             .first == root.end();
}

std::optional<fs::path> ResolveAppFile(const fs::path& root,
                                       const std::string& path) {
  const std::u8string relative(path.begin() + std::min<size_t>(path.size(), 1),
                               path.end());
  std::error_code error;
  fs::path file = fs::weakly_canonical(root / fs::path(relative), error);
  if (error || !Contains(root, file)) {
    return std::nullopt;
  }
  if (fs::is_directory(file, error)) {
    file /= "index.html";
  }
  if (!fs::is_regular_file(file, error)) {
    return std::nullopt;
  }
  return file;
}

std::optional<fs::path> ResolveLocalFile(const std::string& path) {
#if defined(OS_WIN)
  const size_t drive = std::min<size_t>(path.size(), 1);
#else
  const size_t drive = 0;
#endif
  const fs::path file(std::u8string(path.begin() + drive, path.end()));
  std::error_code error;
  if (!file.is_absolute() || !fs::is_regular_file(file, error)) {
    return std::nullopt;
  }
  return file;
}

std::optional<fs::path> ResolveFile(const std::optional<fs::path>& root,
                                    const std::string& url) {
  CefURLParts parts;
  if (!CefParseURL(url, parts)) {
    return std::nullopt;
  }
  const std::string path =
      CefURIDecode(CefString(&parts.path), true,
                   static_cast<cef_uri_unescape_rule_t>(
                       UU_SPACES | UU_URL_SPECIAL_CHARS_EXCEPT_PATH_SEPARATORS))
          .ToString();
  return root ? ResolveAppFile(*root, path) : ResolveLocalFile(path);
}

std::optional<ByteRange> RequestedRange(const std::string& header,
                                        uint64_t size) {
  const std::string prefix = "bytes=";
  if (header.rfind(prefix, 0) != 0 || header.find(',') != std::string::npos) {
    return ByteRange{0, size, false};
  }
  const std::string spec = header.substr(prefix.size());
  const size_t dash = spec.find('-');
  if (dash == std::string::npos) {
    return ByteRange{0, size, false};
  }
  const std::string first = spec.substr(0, dash);
  const std::string last = spec.substr(dash + 1);
  const auto number = [](const std::string& text) -> std::optional<uint64_t> {
    if (text.empty() || !std::all_of(text.begin(), text.end(), [](char digit) {
          return digit >= '0' && digit <= '9';
        })) {
      return std::nullopt;
    }
    return std::strtoull(text.c_str(), nullptr, 10);
  };
  if (first.empty()) {
    const auto suffix = number(last);
    if (!suffix || *suffix == 0 || size == 0) {
      return std::nullopt;
    }
    const uint64_t length = std::min(*suffix, size);
    return ByteRange{size - length, length, true};
  }
  const auto start = number(first);
  if (!start || *start >= size) {
    return std::nullopt;
  }
  uint64_t end = size - 1;
  if (!last.empty()) {
    const auto requested_end = number(last);
    if (!requested_end || *requested_end < *start) {
      return std::nullopt;
    }
    end = std::min(*requested_end, end);
  }
  return ByteRange{*start, end - *start + 1, true};
}

std::string MimeType(const fs::path& file) {
  const std::u8string suffix = file.extension().u8string();
  const std::string extension(
      suffix.begin() + std::min<size_t>(suffix.size(), 1), suffix.end());
  const std::string mime = CefGetMimeType(extension);
  return mime.empty() ? "application/octet-stream" : mime;
}

class FileHandler : public CefResourceHandler {
 public:
  explicit FileHandler(const std::optional<fs::path>& root) : root_(root) {}
  ~FileHandler() override {
    if (file_) {
      std::fclose(file_);
    }
  }

  bool Open(CefRefPtr<CefRequest> request,
            bool& handle_request,
            CefRefPtr<CefCallback> callback) override {
    handle_request = true;
    const auto path = ResolveFile(root_, request->GetURL());
    std::error_code error;
    const uint64_t size = path ? fs::file_size(*path, error) : 0;
    if (!path || error) {
      status_ = 404;
      return true;
    }
    size_ = size;
    const auto range =
        RequestedRange(request->GetHeaderByName("Range").ToString(), size);
    if (!range) {
      status_ = 416;
      return true;
    }
    file_ = OpenFile(*path, range->start);
    if (!file_) {
      status_ = 404;
      return true;
    }
    range_ = *range;
    remaining_ = range->length;
    status_ = range->partial ? 206 : 200;
    mime_type_ = MimeType(*path);
    return true;
  }

  void GetResponseHeaders(CefRefPtr<CefResponse> response,
                          int64_t& response_length,
                          CefString& redirect_url) override {
    response->SetStatus(status_);
    CefResponse::HeaderMap headers{{"Accept-Ranges", "bytes"},
                                   {"Cache-Control", "no-cache"}};
    if (status_ == 206) {
      headers.emplace("Content-Range",
                      "bytes " + std::to_string(range_.start) + "-" +
                          std::to_string(range_.start + range_.length - 1) +
                          "/" + std::to_string(size_));
    } else if (status_ == 416) {
      headers.emplace("Content-Range", "bytes */" + std::to_string(size_));
    }
    if (!root_) {
      headers.emplace("Access-Control-Allow-Origin", kAppOrigin);
    }
    response->SetHeaderMap(headers);
    response->SetMimeType(file_ ? mime_type_ : "text/plain");
    response_length = file_ ? static_cast<int64_t>(range_.length) : 0;
  }

  bool Skip(int64_t bytes_to_skip,
            int64_t& bytes_skipped,
            CefRefPtr<CefResourceSkipCallback> callback) override {
    bytes_skipped = bytes_to_skip;
    return true;
  }

  bool Read(void* data_out,
            int bytes_to_read,
            int& bytes_read,
            CefRefPtr<CefResourceReadCallback> callback) override {
    const size_t wanted = static_cast<size_t>(
        std::min<uint64_t>(static_cast<uint64_t>(bytes_to_read), remaining_));
    bytes_read = file_ && wanted
                     ? static_cast<int>(std::fread(data_out, 1, wanted, file_))
                     : 0;
    remaining_ -= static_cast<uint64_t>(bytes_read);
    return bytes_read > 0;
  }

  void Cancel() override {}

 private:
  const std::optional<fs::path> root_;
  FILE* file_ = nullptr;
  int status_ = 404;
  uint64_t size_ = 0;
  ByteRange range_;
  uint64_t remaining_ = 0;
  std::string mime_type_;
  IMPLEMENT_REFCOUNTING(FileHandler);
};

class FileSchemeHandlerFactory : public CefSchemeHandlerFactory {
 public:
  explicit FileSchemeHandlerFactory(std::optional<fs::path> root)
      : root_(std::move(root)) {}

  CefRefPtr<CefResourceHandler> Create(CefRefPtr<CefBrowser> browser,
                                       CefRefPtr<CefFrame> frame,
                                       const CefString& scheme_name,
                                       CefRefPtr<CefRequest> request) override {
    return new FileHandler(root_);
  }

 private:
  const std::optional<fs::path> root_;
  IMPLEMENT_REFCOUNTING(FileSchemeHandlerFactory);
};

}  // namespace

void RegisterAppScheme(CefRawPtr<CefSchemeRegistrar> registrar) {
  registrar->AddCustomScheme(kScheme, CEF_SCHEME_OPTION_STANDARD |
                                          CEF_SCHEME_OPTION_SECURE |
                                          CEF_SCHEME_OPTION_DISPLAY_ISOLATED |
                                          CEF_SCHEME_OPTION_CORS_ENABLED |
                                          CEF_SCHEME_OPTION_FETCH_ENABLED);
}

void ServeAppFiles(const std::string& root) {
  std::error_code error;
  const fs::path canonical =
      fs::canonical(fs::path(std::u8string(root.begin(), root.end())), error);
  if (error) {
    std::fprintf(stderr, "Sabine CEF: app files are unavailable at %s\n",
                 root.c_str());
    return;
  }
  CefRegisterSchemeHandlerFactory(kScheme, kAppHost,
                                  new FileSchemeHandlerFactory(canonical));
}

void ServeLocalFiles() {
  CefRegisterSchemeHandlerFactory(kScheme, kFileHost,
                                  new FileSchemeHandlerFactory(std::nullopt));
}

}  // namespace sabine_app
