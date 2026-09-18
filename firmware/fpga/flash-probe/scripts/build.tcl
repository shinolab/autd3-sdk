set probe_directory [file normalize [file join [file dirname [info script]] ..]]
set build_directory [file join $probe_directory build]
set part "xc7a200tfbg676-2"

file mkdir $build_directory

read_verilog -sv [list \
    [file join $probe_directory rtl flash_probe_core.sv] \
    [file join $probe_directory rtl flash_probe_iprog.sv] \
    [file join $probe_directory rtl flash_probe_top.sv]]
read_xdc [file join $probe_directory constrs flash_probe.xdc]

set bit_name flash_probe
set next_config_addr ""
if {[llength $argv] > 0 && [string equal [lindex $argv 0] "x1"]} {
    set bit_name flash_probe_x1
}
if {[llength $argv] > 1 && [string equal [lindex $argv 0] "multiboot"]} {
    set bit_name flash_probe_mb
    set next_config_addr [lindex $argv 1]
}

synth_design -top flash_probe_top -part $part
opt_design
place_design
route_design

foreach primitive {ICAPE2 STARTUPE2 BSCANE2} {
    set cells [get_cells -hierarchical -filter "REF_NAME == $primitive"]
    if {[llength $cells] != 1} {
        error "expected exactly one $primitive in the routed design, found [llength $cells]"
    }
}

report_timing_summary -file [file join $build_directory timing.rpt]
report_drc -file [file join $build_directory drc.rpt]

set wns [get_property SLACK [get_timing_paths -max_paths 1 -nworst 1 -setup]]
if {$wns eq "" || $wns < 0} {
    error "timing not met (WNS=$wns); see [file join $build_directory timing.rpt]"
}

if {[string equal $bit_name flash_probe_x1]} {
    set_property CONFIG_MODE SPIx1 [current_design]
    set_property BITSTREAM.CONFIG.SPI_BUSWIDTH 1 [current_design]
}

if {![string equal $next_config_addr ""]} {
    set_property BITSTREAM.CONFIG.NEXT_CONFIG_ADDR $next_config_addr [current_design]
    puts "NEXT_CONFIG_ADDR = $next_config_addr"
}

set bit_file_path [file join $build_directory $bit_name.bit]
write_bitstream -force $bit_file_path
puts "bitstream written: $bit_file_path (WNS=$wns)"
