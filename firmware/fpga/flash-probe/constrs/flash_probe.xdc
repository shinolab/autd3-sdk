set_property PACKAGE_PIN H21 [get_ports MRCC_25P6M]
set_property IOSTANDARD LVCMOS33 [get_ports MRCC_25P6M]
set_property PULLTYPE PULLUP [get_ports MRCC_25P6M]
create_clock -period 39.063 -name sys_clk [get_ports MRCC_25P6M]

create_clock -period 33.333 -name jtag_tck [get_pins bscan/TCK]
set_clock_groups -asynchronous -group [get_clocks sys_clk] -group [get_clocks jtag_tck]

set_property PACKAGE_PIN P18 [get_ports FLASH_CS_N]
set_property PACKAGE_PIN R14 [get_ports FLASH_MOSI]
set_property PACKAGE_PIN R15 [get_ports FLASH_MISO]
set_property PACKAGE_PIN P14 [get_ports FLASH_WP_N]
set_property PACKAGE_PIN N14 [get_ports FLASH_HOLD_N]
set_property IOSTANDARD LVCMOS33 [get_ports {FLASH_CS_N FLASH_MOSI FLASH_MISO FLASH_WP_N FLASH_HOLD_N}]

set_false_path -to [get_ports {FLASH_CS_N FLASH_MOSI FLASH_WP_N FLASH_HOLD_N}]
set_false_path -from [get_ports FLASH_MISO]

set_property CFGBVS VCCO [current_design]
set_property CONFIG_VOLTAGE 3.3 [current_design]
set_property CONFIG_MODE SPIx4 [current_design]
set_property BITSTREAM.GENERAL.COMPRESS TRUE [current_design]
set_property BITSTREAM.CONFIG.UNUSEDPIN Pulldown [current_design]
set_property BITSTREAM.CONFIG.CONFIGFALLBACK ENABLE [current_design]
