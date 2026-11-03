#include "app/file_range.h"

#include <algorithm>
#include <cstdlib>

namespace sabine_app {
namespace {

FILE* OpenAt(const std::filesystem::path& path, uint64_t offset) {
#if defined(_WIN32)
  FILE* file = _wfopen(path.c_str(), L"rb");
  const bool positioned =
      file && _fseeki64(file, static_cast<int64_t>(offset), SEEK_SET) == 0;
#else
  FILE* file = std::fopen(path.c_str(), "rb");
  const bool positioned =
      file && fseeko(file, static_cast<off_t>(offset), SEEK_SET) == 0;
#endif
  if (file && !positioned) {
    std::fclose(file);
    return nullptr;
  }
  return file;
}

std::optional<uint64_t> Number(const std::string& text) {
  if (text.empty() || !std::all_of(text.begin(), text.end(), [](char digit) {
        return digit >= '0' && digit <= '9';
      })) {
    return std::nullopt;
  }
  return std::strtoull(text.c_str(), nullptr, 10);
}

}  // namespace

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
  if (first.empty()) {
    const auto suffix = Number(last);
    if (!suffix || *suffix == 0 || size == 0) {
      return std::nullopt;
    }
    const uint64_t length = std::min(*suffix, size);
    return ByteRange{size - length, length, true};
  }
  const auto start = Number(first);
  if (!start || *start >= size) {
    return std::nullopt;
  }
  uint64_t end = size - 1;
  if (!last.empty()) {
    const auto requested_end = Number(last);
    if (!requested_end || *requested_end < *start) {
      return std::nullopt;
    }
    end = std::min(*requested_end, end);
  }
  return ByteRange{*start, end - *start + 1, true};
}

FileRangeReader::~FileRangeReader() {
  if (file_) {
    std::fclose(file_);
  }
}

bool FileRangeReader::Open(const std::filesystem::path& path,
                           const ByteRange& range) {
  file_ = OpenAt(path, range.start);
  range_ = range;
  remaining_ = range.length;
  return file_ != nullptr;
}

bool FileRangeReader::SkipToRangeStart(uint64_t bytes) {
  return file_ && bytes == range_.start;
}

size_t FileRangeReader::Read(void* data, size_t size) {
  const size_t wanted =
      static_cast<size_t>(std::min<uint64_t>(size, remaining_));
  const size_t read = file_ && wanted ? std::fread(data, 1, wanted, file_) : 0;
  remaining_ -= read;
  return read;
}

}  // namespace sabine_app
