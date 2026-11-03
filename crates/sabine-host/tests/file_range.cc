#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include "app/file_range.h"

namespace {

constexpr uint64_t kFileSize = 5ull << 30;
constexpr uint64_t kFarOffset = (3ull << 30) + 7;
constexpr char kFarMarker[] = "far into the file";
constexpr char kEndMarker[] = "the last bytes!";

int failures = 0;

void Expect(bool condition, const std::string& description) {
  if (!condition) {
    std::cerr << "failed: " << description << std::endl;
    ++failures;
  }
}

void WriteAt(const std::filesystem::path& path,
             uint64_t offset,
             const std::string& text) {
  std::fstream file(path, std::ios::in | std::ios::out | std::ios::binary);
  file.seekp(static_cast<std::streamoff>(offset));
  file.write(text.data(), static_cast<std::streamsize>(text.size()));
}

std::string ReadAll(sabine_app::FileRangeReader& reader) {
  std::string body;
  std::vector<char> chunk(64 * 1024);
  while (const size_t read = reader.Read(chunk.data(), chunk.size())) {
    body.append(chunk.data(), read);
  }
  return body;
}

std::string Serve(const std::filesystem::path& path,
                  const std::string& header,
                  bool chromium_skips) {
  const auto range = sabine_app::RequestedRange(header, kFileSize);
  sabine_app::FileRangeReader reader;
  if (!range || !reader.Open(path, *range)) {
    return "<not served>";
  }
  if (chromium_skips && !reader.SkipToRangeStart(range->start)) {
    return "<skip refused>";
  }
  return ReadAll(reader);
}

}  // namespace

int main(int argc, char** argv) {
  const std::filesystem::path path =
      std::filesystem::path(argv[1]) / "sparse.bin";
  std::ofstream(path, std::ios::binary).close();
  std::filesystem::resize_file(path, kFileSize);
  const std::string far(kFarMarker);
  const std::string end(kEndMarker);
  WriteAt(path, kFarOffset, far);
  WriteAt(path, kFileSize - end.size(), end);

  const std::string far_start = std::to_string(kFarOffset);
  Expect(Serve(path,
               "bytes=" + far_start + "-" +
                   std::to_string(kFarOffset + far.size() - 1),
               true) == far,
         "a bounded range past 4 GiB starts at its first byte");

  const uint64_t tail = kFileSize - 100000;
  const std::string open_ended =
      Serve(path, "bytes=" + std::to_string(tail) + "-", true);
  Expect(open_ended.size() == 100000,
         "an open-ended range runs to the end of the file");
  Expect(open_ended.ends_with(end),
         "an open-ended range ends with the file's last bytes");

  Expect(Serve(path, "bytes=-" + std::to_string(end.size()), false) == end,
         "a suffix range is the file's last bytes");

  Expect(!sabine_app::RequestedRange("bytes=" + std::to_string(kFileSize) + "-",
                                     kFileSize),
         "a range starting past the end is unsatisfiable");

  std::filesystem::remove(path);
  return failures == 0 ? 0 : 1;
}
