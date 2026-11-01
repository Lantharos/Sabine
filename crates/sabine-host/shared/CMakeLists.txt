cmake_minimum_required(VERSION 3.21)

project(sabine_cef_host)

set(CMAKE_CONFIGURATION_TYPES Debug Release)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(SABINE_CEF_API_VERSION "15400" CACHE STRING "Stable CEF API used by Sabine")
add_compile_definitions(CEF_API_VERSION=${SABINE_CEF_API_VERSION})

if(NOT CEF_ROOT OR NOT SABINE_HOST_OUTPUT_DIR)
  message(FATAL_ERROR "CEF_ROOT and SABINE_HOST_OUTPUT_DIR are required")
endif()

# CEF macros interpolate CEF_LIB_* into CMake strings; backslashes become escapes.
file(TO_CMAKE_PATH "${CEF_ROOT}" CEF_ROOT)

if(WIN32 AND CMAKE_CXX_COMPILER_ID STREQUAL "GNU")
  message(FATAL_ERROR
    "sabine-host on Windows requires MSVC. Official CEF binaries cannot be linked with MinGW/MSYS.")
endif()

set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CEF_ROOT}/cmake")
find_package(CEF REQUIRED)

if(OS_MAC)
  enable_language(OBJCXX)
  set(CMAKE_OBJCXX_STANDARD 20)
  set(CMAKE_OBJCXX_STANDARD_REQUIRED ON)
endif()

add_subdirectory(${CEF_LIBCEF_DLL_WRAPPER_PATH} libcef_dll_wrapper)

file(GLOB_RECURSE SABINE_HOST_SOURCES
  RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}"
  CONFIGURE_DEPENDS
  "*.cc" "*.h" "*.mm"
)

# Sources in a folder named after a platform build only on that platform.
foreach(_platform linux:OS_LINUX macos:OS_MAC windows:OS_WINDOWS)
  string(REPLACE ":" ";" _platform "${_platform}")
  list(GET _platform 0 _folder)
  list(GET _platform 1 _flag)
  if(NOT ${_flag})
    list(FILTER SABINE_HOST_SOURCES EXCLUDE REGEX "(^|/)${_folder}/")
  endif()
endforeach()

if(OS_WINDOWS)
  list(REMOVE_ITEM SABINE_HOST_SOURCES osr/paint/shared_memory_posix.cc)
endif()

if(OS_MAC)
  list(APPEND SABINE_HOST_SOURCES macos/sabine.icns)
  set_source_files_properties(macos/sabine.icns PROPERTIES MACOSX_PACKAGE_LOCATION Resources)
endif()

if(OS_WINDOWS)
  add_library(sabine-host SHARED ${SABINE_HOST_SOURCES})
  SET_LIBRARY_TARGET_PROPERTIES(sabine-host)
elseif(OS_MAC)
  add_executable(sabine-host MACOSX_BUNDLE ${SABINE_HOST_SOURCES})
else()
  add_executable(sabine-host ${SABINE_HOST_SOURCES})
endif()

if(NOT OS_WINDOWS)
  SET_EXECUTABLE_TARGET_PROPERTIES(sabine-host)
endif()
target_include_directories(sabine-host PRIVATE
  "${CMAKE_CURRENT_SOURCE_DIR}"
  "${CEF_ROOT}"
)
add_dependencies(sabine-host libcef_dll_wrapper)
if(OS_MAC)
  target_link_libraries(sabine-host libcef_dll_wrapper ${CEF_STANDARD_LIBS}
    "-framework Metal"
  )
else()
  ADD_LOGICAL_TARGET("libcef_lib" "${CEF_LIB_DEBUG}" "${CEF_LIB_RELEASE}")
  target_link_libraries(sabine-host libcef_lib libcef_dll_wrapper ${CEF_STANDARD_LIBS})
endif()
if(OS_WINDOWS)
  target_link_libraries(sabine-host ws2_32 d3d11 d3d12 dxgi kernelbase shell32)
endif()
if(OS_LINUX)
  find_path(SABINE_VULKAN_INCLUDE_DIR vulkan/vulkan.h REQUIRED)
  target_include_directories(sabine-host PRIVATE "${SABINE_VULKAN_INCLUDE_DIR}")
  target_link_libraries(sabine-host ${CMAKE_DL_LIBS})
  set_target_properties(sabine-host PROPERTIES INSTALL_RPATH "$ORIGIN")
  set_target_properties(sabine-host PROPERTIES BUILD_WITH_INSTALL_RPATH TRUE)
endif()
if(OS_MAC)
  set(EXECUTABLE_NAME "sabine-host")
  set(PRODUCT_NAME "sabine-host")
  set(PRODUCT_BUNDLE_IDENTIFIER "dev.sabine.host")
  set_target_properties(sabine-host PROPERTIES
    MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/macos/Info.plist.in"
  )
endif()
set_target_properties(sabine-host PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${SABINE_HOST_OUTPUT_DIR}")
set_target_properties(sabine-host PROPERTIES RUNTIME_OUTPUT_DIRECTORY_DEBUG "${SABINE_HOST_OUTPUT_DIR}")
set_target_properties(sabine-host PROPERTIES RUNTIME_OUTPUT_DIRECTORY_RELEASE "${SABINE_HOST_OUTPUT_DIR}")

if(OS_WINDOWS)
  add_custom_command(TARGET sabine-host POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy "${CEF_BINARY_DIR}/bootstrap.exe" "${SABINE_HOST_OUTPUT_DIR}/sabine-host.exe"
    COMMAND ${CMAKE_COMMAND} -E copy "${CEF_BINARY_DIR}/chrome_elf.dll" "${SABINE_HOST_OUTPUT_DIR}/chrome_elf.dll"
    VERBATIM)
endif()

if(OS_MAC)
  set(SABINE_HOST_APP "${SABINE_HOST_OUTPUT_DIR}/sabine-host.app")

  set(_sabine_helper_index 0)
  foreach(_suffix_list ${CEF_HELPER_APP_SUFFIXES})
    string(REPLACE ":" ";" _suffix_list ${_suffix_list})
    list(GET _suffix_list 0 _name_suffix)
    list(GET _suffix_list 2 _plist_suffix)
    set(_helper_name "sabine-host Helper${_name_suffix}")
    set(_helper_app "${SABINE_HOST_APP}/Contents/Frameworks/${_helper_name}.app")
    set(_helper_plist "${CMAKE_CURRENT_BINARY_DIR}/sabine-host-helper-${_sabine_helper_index}.plist")
    math(EXPR _sabine_helper_index "${_sabine_helper_index} + 1")
    file(WRITE "${_helper_plist}"
      "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
      "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n"
      "<plist version=\"1.0\"><dict>\n"
      "<key>CFBundleExecutable</key><string>${_helper_name}</string>\n"
      "<key>CFBundleIdentifier</key><string>dev.sabine.host.helper${_plist_suffix}</string>\n"
      "<key>CFBundleName</key><string>${_helper_name}</string>\n"
      "<key>CFBundlePackageType</key><string>APPL</string>\n"
      "<key>LSBackgroundOnly</key><true/>\n"
      "</dict></plist>\n")
    add_custom_command(TARGET sabine-host POST_BUILD
      COMMAND ${CMAKE_COMMAND} -E make_directory "${_helper_app}/Contents/MacOS"
      COMMAND ${CMAKE_COMMAND} -E copy "$<TARGET_FILE:sabine-host>" "${_helper_app}/Contents/MacOS/${_helper_name}"
      COMMAND ${CMAKE_COMMAND} -E copy "${_helper_plist}" "${_helper_app}/Contents/Info.plist"
      VERBATIM)
  endforeach()
endif()
