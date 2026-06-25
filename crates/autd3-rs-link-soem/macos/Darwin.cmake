# Derived from SOEM `cmake/Linux.cmake`.
#
# SOEM is dual-licensed under GPLv3 and a commercial license; this crate uses
# it under GPLv3 (the GPL-3.0 text ships with this crate as COPYING). The
# canonical SOEM source and its LICENSE.md live upstream:
# https://github.com/OpenEtherCATsociety/SOEM
#
# Modification for autd3-rs-link-soem (2026-06): targets the `osal/macosx` /
# `oshw/macosx` platform layer and links libpcap instead of `rt`. vendored
# SOEM 2.x has no macOS port in core (only an unmaintained one under
# `contrib/`), so this file is supplied by the crate and injected at build
# time by `build.rs`.

target_sources(soem PRIVATE
  osal/macosx/osal.c
  osal/macosx/osal_defs.h
  oshw/macosx/oshw.c
  oshw/macosx/oshw.h
  oshw/macosx/nicdrv.c
  oshw/macosx/nicdrv.h
)

target_include_directories(soem PUBLIC
  $<BUILD_INTERFACE:${SOEM_SOURCE_DIR}/osal/macosx>
  $<BUILD_INTERFACE:${SOEM_SOURCE_DIR}/oshw/macosx>
  $<INSTALL_INTERFACE:include/soem>
)

foreach(target IN ITEMS
    soem
    ec_sample
    eepromtool
    eni_test
    firm_update
    simple_ng
    slaveinfo)
  if (TARGET ${target})
    target_compile_options(${target} PRIVATE
      -Wall
      -Wextra
    )
  endif()
endforeach()

target_link_libraries(soem PUBLIC pcap pthread)

install(FILES
  osal/macosx/osal_defs.h
  oshw/macosx/nicdrv.h
  DESTINATION include/soem
)
