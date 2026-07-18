set project_directory [file normalize [file join [file dirname [info script]] ..]]
set project_name      "autd3-fpga"

cd $project_directory
open_project [file join $project_directory "$project_name.xpr"]

set_property -name {xsim.compile.tcl.pre} -value {} -objects [get_filesets sim_1]
set_property -name {xsim.simulate.xsim.more_options} -value {-sv_seed random} -objects [get_filesets sim_1]

# power.tcl sets these to capture a SAIF and they persist in the .xpr. A scope left over from
# it does not exist in most testbenches, which makes them stop before $finish.
set_property -name {xsim.simulate.saif_scope} -value {} -objects [get_filesets sim_1]
set_property -name {xsim.simulate.saif} -value {} -objects [get_filesets sim_1]
set_property -name {xsim.simulate.saif_all_signals} -value {false} -objects [get_filesets sim_1]

proc collect_tbs {project_directory} {
    set files [concat \
        [glob -nocomplain [file join $project_directory rtl/sim_1/new/sim_*.sv]] \
        [glob -nocomplain [file join $project_directory rtl/sim_1/new/*/sim_*.sv]]]
    set tbs {}
    foreach f $files {
        set name [file rootname [file tail $f]]
        if {[string match "sim_helper_*" $name]} {
            continue
        }
        lappend tbs $name
    }
    return [lsort -unique $tbs]
}

set requested $argv
if {[llength $requested] > 0} {
    set tbs $requested
} else {
    set tbs [collect_tbs $project_directory]
}

set log_path [file join $project_directory "$project_name.sim" sim_1 behav xsim simulate.log]

set results {}
set n_fail 0
foreach tb $tbs {
    puts "==================== SIM: $tb ===================="
    set_property top $tb [get_filesets sim_1]
    set_property top_lib xil_defaultlib [get_filesets sim_1]

    set ok 1
    set reason "ok"
    if {[catch {launch_simulation} msg]} {
        set ok 0
        set reason "elaboration/compile error"
        puts "launch_simulation failed: $msg"
    } else {
        set logf $log_path
        if {![file exists $logf]} {
            set found [glob -nocomplain [file join $project_directory "$project_name.sim" sim_1 behav xsim simulate.log]]
            if {[llength $found] > 0} {
                set logf [lindex $found 0]
            }
        }
        if {![file exists $logf]} {
            set ok 0
            set reason "no simulate.log"
        } else {
            set fh [open $logf r]
            set content [read $fh]
            close $fh
            if {[string first "but actual is" $content] >= 0} {
                set ok 0
                set reason "assertion failed"
            } elseif {[string first "FATAL_ERROR" $content] >= 0} {
                set ok 0
                set reason "fatal error"
            } elseif {[string first "\$finish called" $content] < 0} {
                set ok 0
                set reason "did not reach \$finish"
            }
        }
        close_sim -quiet
    }

    if {$ok} {
        puts "RESULT: $tb PASS"
        lappend results [list $tb PASS $reason]
    } else {
        incr n_fail
        puts "RESULT: $tb FAIL ($reason)"
        lappend results [list $tb FAIL $reason]
    }
}

puts ""
puts "==================== SUMMARY ===================="
foreach r $results {
    puts [format "  %-6s %-32s %s" [lindex $r 1] [lindex $r 0] [lindex $r 2]]
}
puts [format "  %d passed, %d failed, %d total" [expr {[llength $results] - $n_fail}] $n_fail [llength $results]]

close_project

if {$n_fail > 0} {
    exit 1
}
