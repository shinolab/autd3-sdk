cmake_minimum_required(VERSION 3.21)

if(${CMAKE_SOURCE_DIR} STREQUAL ${CMAKE_BINARY_DIR})
  message(FATAL_ERROR "In-source builds not allowed. Make a build/ directory and run cmake from there.")
endif()

project(autd3-rs-cpu-firmware-test LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

option(COVERAGE "Take coverage" OFF)
if(COVERAGE)
  set(CMAKE_C_FLAGS   "${CMAKE_C_FLAGS}   --coverage -fno-inline")
  set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} --coverage -fno-inline")
  set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} --coverage")
endif()

include(${PROJECT_SOURCE_DIR}/gtest.cmake)

enable_testing()

set(FW_INC ${PROJECT_SOURCE_DIR}/../inc)
set(FW_SRC ${PROJECT_SOURCE_DIR}/../src)

add_executable(test_autd3-rs-cpu
  main.cpp
  port_host.cpp
  test_proto.cpp
  ${FW_SRC}/app.c
  ${FW_SRC}/proto.c
  ${FW_SRC}/fpga.c
  ${FW_SRC}/cmd/xor_hash.c
  ${FW_SRC}/cmd/write_pattern.c
  ${FW_SRC}/cmd/write_pattern_compressed.c
  ${FW_SRC}/cmd/write_mod.c
  ${FW_SRC}/cmd/config_mod.c
  ${FW_SRC}/cmd/config_pattern.c
  ${FW_SRC}/cmd/change_mod_bank.c
  ${FW_SRC}/cmd/change_pattern_bank.c
  ${FW_SRC}/cmd/clear.c
  ${FW_SRC}/cmd/sync.c
  ${FW_SRC}/cmd/set_mode.c
  ${FW_SRC}/cmd/silencer.c
  ${FW_SRC}/cmd/force_fan.c
  ${FW_SRC}/cmd/gpio_in.c
  ${FW_SRC}/cmd/phase_corr.c
  ${FW_SRC}/cmd/output_mask.c
  ${FW_SRC}/cmd/pwe.c
  ${FW_SRC}/cmd/gpio_out.c
)

target_include_directories(test_autd3-rs-cpu PRIVATE
  ${FW_INC}
  ${FW_SRC}
)

target_link_libraries(test_autd3-rs-cpu PRIVATE gtest_main)

include(GoogleTest)
gtest_discover_tests(test_autd3-rs-cpu)
