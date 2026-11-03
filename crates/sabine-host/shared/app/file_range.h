#ifndef SABINE_CEF_HOST_APP_FILE_RANGE_H_
#define SABINE_CEF_HOST_APP_FILE_RANGE_H_

#include <cstdint>
#include <cstdio>
#include <filesystem>
#include <optional>
#include <string>

namespace sabine_app {

struct ByteRange {
  uint64_t start = 0;
  uint64_t length = 0;
  bool partial = false;
};

/// The bytes of a |size|-byte file a Range |header| asks for: the whole file
/// when there is no single range, nothing when the range can't be satisfied.
std::optional<ByteRange> RequestedRange(const std::string& header,
                                        uint64_t size);

/// Reads one byte range of a file. Chromium skips to the first byte of a
/// range it asked for before reading, so a reader starts there and accepts
/// exactly that skip.
class FileRangeReader {
 public:
  FileRangeReader() = default;
  FileRangeReader(const FileRangeReader&) = delete;
  FileRangeReader& operator=(const FileRangeReader&) = delete;
  ~FileRangeReader();

  bool Open(const std::filesystem::path& path, const ByteRange& range);
  bool SkipToRangeStart(uint64_t bytes);
  size_t Read(void* data, size_t size);

  bool is_open() const { return file_ != nullptr; }
  const ByteRange& range() const { return range_; }

 private:
  FILE* file_ = nullptr;
  ByteRange range_;
  uint64_t remaining_ = 0;
};

}  // namespace sabine_app

#endif
