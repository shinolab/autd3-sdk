set project_directory [file normalize [file join [file dirname [info script]] ..]]
set project_name      "autd3-fpga"
set synth_run         "synth_alter"
set impl_run          "impl_alter_def"

set golden_usr_access "0x474F4C44"

set jobs            1
set force           0
set barrier         ""
set barrier_address ""
set update_address  ""
if {[llength $argv] > 0} {
    set jobs [lindex $argv 0]
}
foreach arg [lrange $argv 1 end] {
    if {[string equal $arg "force"]} {
        set force 1
    } elseif {[string match "barrier=*" $arg]} {
        set barrier [string range $arg 8 end]
    } elseif {[string match "barrier_address=*" $arg]} {
        set barrier_address [string range $arg 16 end]
    } elseif {[string match "update_address=*" $arg]} {
        set update_address [string range $arg 15 end]
    }
}

if {![file exists $barrier] || [string equal $barrier_address ""] || [string equal $update_address ""]} {
    error "barrier image or flash addresses missing (barrier='$barrier' barrier_address='$barrier_address' update_address='$update_address'); run through cargo xtask fpga build"
}
set barrier [file normalize $barrier]
set golden_next_config_addr $barrier_address

cd $project_directory
open_project [file join $project_directory "$project_name.xpr"]
source [file join $project_directory scripts sync_sources.tcl]
sync_project_sources $project_directory

set bit_file_path [file join $project_directory "$project_name.runs" $impl_run "top.bit"]

if {$force || ![file exists $bit_file_path]} {
    if {$force} {
        reset_run $synth_run
    }
    if {![string equal [get_property PROGRESS [get_runs $synth_run]] "100%"]} {
        launch_runs $synth_run -jobs $jobs
        wait_on_run $synth_run
        if {![string equal [get_property PROGRESS [get_runs $synth_run]] "100%"]} {
            error "synthesis failed: see $project_name.runs/$synth_run/runme.log"
        }
    }
    launch_runs $impl_run -to_step write_bitstream -jobs $jobs
    wait_on_run $impl_run
    if {![string equal [get_property PROGRESS [get_runs $impl_run]] "100%"]} {
        error "implementation failed: see $project_name.runs/$impl_run/runme.log"
    }
} else {
    puts "reusing existing bitstream: $bit_file_path"
}

if {![file exists $bit_file_path]} {
    error "bitstream not found: $bit_file_path"
}

set golden_bit_path [file join $project_directory "$project_name-golden.bit"]
set golden_stamp_path [file join $project_directory "$project_name-golden.stamp"]
set golden_stamp "next_config_addr=$golden_next_config_addr usr_access=$golden_usr_access"
set golden_fresh 0
if {[file exists $golden_bit_path] && [file exists $golden_stamp_path]
    && [file mtime $golden_bit_path] >= [file mtime $bit_file_path]} {
    set fh [open $golden_stamp_path r]
    set golden_fresh [string equal [string trim [read $fh]] $golden_stamp]
    close $fh
}
if {$force || !$golden_fresh} {
    open_run $impl_run
    set_property BITSTREAM.CONFIG.NEXT_CONFIG_ADDR $golden_next_config_addr [current_design]
    set_property BITSTREAM.CONFIG.USR_ACCESS $golden_usr_access [current_design]
    write_bitstream -force $golden_bit_path
    close_design
    set fh [open $golden_stamp_path w]
    puts $fh $golden_stamp
    close $fh
} else {
    puts "reusing existing golden bitstream: $golden_bit_path"
}

set mcs_file_path [file join $project_directory "$project_name.mcs"]
write_cfgmem -format mcs -size 16 -interface SPIx4 -loadbit "up 0x00000000 $golden_bit_path up $update_address $bit_file_path" -loaddata "up $barrier_address $barrier" -force -file $mcs_file_path
close_project

puts "mcs written: $mcs_file_path"
