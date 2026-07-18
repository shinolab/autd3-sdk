set project_directory [file normalize [file join [file dirname [info script]] ..]]
set project_name      "autd3-fpga"

cd $project_directory

set tb [lindex $argv 0]
if {$tb eq ""} {
    set tb "sim_emission_focus"
}
set scope_inst [lindex $argv 1]
if {$scope_inst eq ""} {
    set scope_inst "emission"
}
set impl_run [lindex $argv 2]
if {$impl_run eq ""} {
    set impl_run "impl_alter_def"
}
set reuse_saif [expr {[lindex $argv 3] eq "reuse"}]

set sim_dir  [file join $project_directory "$project_name.sim" sim_1 behav xsim]
set saif_out [file join $sim_dir "power.saif"]

if {$reuse_saif && [file exists $saif_out]} {
    puts "==================== SAIF: reusing $saif_out ===================="
} else {
    puts "==================== SAIF: $tb/$scope_inst ===================="
    open_project [file join $project_directory "$project_name.xpr"]

    set_property -name {xsim.compile.tcl.pre}           -value {}              -objects [get_filesets sim_1]
    set_property -name {xsim.simulate.saif_scope}       -value "$tb/$scope_inst" -objects [get_filesets sim_1]
    set_property -name {xsim.simulate.saif}             -value "power.saif"      -objects [get_filesets sim_1]
    set_property -name {xsim.simulate.saif_all_signals} -value {true}            -objects [get_filesets sim_1]

    set_property top $tb [get_filesets sim_1]
    set_property top_lib xil_defaultlib [get_filesets sim_1]

    set sim_failed [catch {launch_simulation} sim_msg]
    close_sim -quiet

    # These properties persist in the .xpr, so a stale saif_scope would make every later
    # `fpga sim` log a scope that does not exist in its testbench and never reach $finish.
    # Always hand the project back the way we found it, even if the simulation failed.
    set_property -name {xsim.simulate.saif_scope}       -value {}      -objects [get_filesets sim_1]
    set_property -name {xsim.simulate.saif}             -value {}      -objects [get_filesets sim_1]
    set_property -name {xsim.simulate.saif_all_signals} -value {false} -objects [get_filesets sim_1]
    close_project

    if {$sim_failed} {
        puts "ERROR: launch_simulation failed: $sim_msg"
        exit 1
    }
}

if {![file exists $saif_out]} {
    puts "ERROR: SAIF not generated at $saif_out"
    exit 1
}
puts "SAIF: $saif_out ([file size $saif_out] bytes)"

# The SAIF is rooted at the testbench, which instantiates the DUT directly:
#   sim_emission_focus / emission / ...
# The netlist nests it one level deeper:
#   top / main / emission / ...
# read_saif can only strip a prefix, never add one, and it ignores current_instance,
# so splice the missing level into the SAIF and strip the testbench root instead.
set saif_fixed [file join $sim_dir "power_reparented.saif"]
set parent "main"

set fin  [open $saif_out r]
set fout [open $saif_fixed w]
set spliced 0
set depth 0
while {[gets $fin line] >= 0} {
    if {!$spliced && [regexp "^(\\s*)\\(INSTANCE\\s+$tb\\s*$" $line -> indent]} {
        puts $fout $line
        puts $fout "$indent   (INSTANCE  $parent"
        set spliced 1
        set depth 1
        continue
    }
    puts $fout $line
}
close $fin
if {!$spliced} {
    close $fout
    puts "ERROR: could not find the testbench root instance '$tb' in the SAIF"
    exit 1
}
close $fout

# Re-balance: the spliced level needs one extra closing paren before the root closes.
set fh [open $saif_fixed r]
set data [read $fh]
close $fh
set data [string trimright $data]
if {![regexp {^(.*)\n(\s*)\)\n(\s*)\)$} $data -> body close_root close_file]} {
    puts "ERROR: unexpected SAIF tail; cannot re-balance parentheses"
    exit 1
}
set fh [open $saif_fixed w]
puts $fh "$body\n$close_root   )\n$close_root)\n$close_file)"
close $fh

set dcp [file join $project_directory "$project_name.runs" $impl_run "top_routed.dcp"]
if {![file exists $dcp]} {
    puts "ERROR: $dcp not found. Run `cargo xtask fpga build` first."
    exit 1
}

set rpt [file join $project_directory "$project_name.runs" $impl_run "top_power_saif.rpt"]

puts "==================== POWER: $parent/$scope_inst ===================="
open_checkpoint $dcp
read_saif -strip_path $tb $saif_fixed
report_power -file $rpt
puts "REPORT: $rpt"
