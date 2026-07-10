set project_directory [file normalize [file dirname [info script]]]
set project_name      "autd3-fpga"
set synth_run         "synth_alter"
set impl_run          "impl_alter_def"

set jobs  1
set force 0
if {[llength $argv] > 0} {
    set jobs [lindex $argv 0]
}
if {[llength $argv] > 1 && [string equal [lindex $argv 1] "force"]} {
    set force 1
}

cd $project_directory
open_project [file join $project_directory "$project_name.xpr"]

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

set mcs_file_path [file join $project_directory "$project_name.mcs"]
write_cfgmem -format mcs -size 16 -interface SPIx4 -loadbit "up 0x00000000 $bit_file_path" -force -file $mcs_file_path
close_project

puts "mcs written: $mcs_file_path"
