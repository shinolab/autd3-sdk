set action [lindex $argv 0]

set user1_ir 02
set ir_bits 6
set dr_bits 80
set signature 45
set idle_tdi [string repeat 0 20]

proc check_signature {tdo} {
    global signature
    set value [expr {"0x$tdo"}]
    if {(($value >> 74) & 0x3F) != $signature} {
        error "flash-probe bitstream is not loaded (TDO=$tdo)"
    }
    return $value
}

proc open_user1 {} {
    global user1_ir ir_bits
    close_hw_target
    open_hw_target -jtag_mode on
    run_state_hw_jtag RESET
    run_state_hw_jtag IDLE
    scan_ir_hw_jtag $ir_bits -tdi $user1_ir
}

proc report_status {device} {
    refresh_hw_device -update_hw_probes false $device
    foreach p [lsort [list_property $device]] {
        if {[regexp {^REGISTER\.} $p]} {
            puts "STATUS $p = [get_property $p $device]"
        }
    }
}

open_hw_manager
connect_hw_server -allow_non_jtag
open_hw_target

set devices [get_hw_devices]
if {[llength $devices] != 1} {
    error "expected exactly one device on the JTAG chain, found: $devices"
}
set device [lindex $devices 0]
if {![string match "xc7a200t*" $device]} {
    error "unexpected JTAG device: $device"
}

switch $action {
    load {
        set bit_file_path [lindex $argv 1]
        set_property PROBES.FILE {} $device
        set_property FULL_PROBES.FILE {} $device
        set_property PROGRAM.FILE $bit_file_path $device
        program_hw_devices $device
        puts "LOADED $bit_file_path"
    }
    cmd {
        set requests [lrange $argv 1 end]
        if {[llength $requests] == 0 || [llength $requests] % 2 != 0} {
            error "cmd expects <tdi> <timeout_ms> pairs"
        }

        open_user1

        foreach {tdi timeout_ms} $requests {
            scan_dr_hw_jtag $dr_bits -tdi $tdi
            set elapsed 0
            while {1} {
                after 100
                incr elapsed 100
                set tdo [scan_dr_hw_jtag $dr_bits -tdi $idle_tdi]
                set value [check_signature $tdo]
                if {(($value >> 73) & 1) == 1 && (($value >> 72) & 1) == 0} {
                    puts "RESULT $tdo"
                    break
                }
                if {$elapsed >= $timeout_ms} {
                    error "timed out waiting for the command (TDO=$tdo)"
                }
            }
        }
        close_hw_target
    }
    iprog {
        set tdi [lindex $argv 1]
        set wait_ms [lindex $argv 2]

        open_user1
        set value [check_signature [scan_dr_hw_jtag $dr_bits -tdi $idle_tdi]]
        if {(($value >> 72) & 1) == 1} {
            error "flash-probe is busy"
        }
        scan_dr_hw_jtag $dr_bits -tdi $tdi
        puts "IPROG sent; waiting $wait_ms ms"
        close_hw_target

        after $wait_ms
        open_hw_target
        set device [lindex [get_hw_devices] 0]
        report_status $device
    }
    status {
        report_status $device
    }
    stage {
        set mcs_file_path [lindex $argv 1]
        set probe_bit_file_path [lindex $argv 2]
        set pairs [lrange $argv 3 end]
        if {[llength $pairs] == 0 || [llength $pairs] % 2 != 0} {
            error "stage expects <address> <bin> pairs"
        }
        set loaddata {}
        foreach {address bin_file_path} $pairs {
            append loaddata "up $address $bin_file_path "
        }

        write_cfgmem -format mcs -size 16 -interface SPIx4 -loaddata [string trim $loaddata] -force -file $mcs_file_path

        create_hw_cfgmem -hw_device $device [lindex [get_cfgmem_parts {mt25ql128-spi-x1_x2_x4}] 0]
        set cfgmem [get_property PROGRAM.HW_CFGMEM $device]
        set_property PROGRAM.ADDRESS_RANGE {use_file} $cfgmem
        set_property PROGRAM.FILES [list $mcs_file_path] $cfgmem
        set_property PROGRAM.PRM_FILE {} $cfgmem
        set_property PROGRAM.UNUSED_PIN_TERMINATION {pull-none} $cfgmem
        set_property PROGRAM.BLANK_CHECK 0 $cfgmem
        set_property PROGRAM.ERASE 1 $cfgmem
        set_property PROGRAM.CFG_PROGRAM 1 $cfgmem
        set_property PROGRAM.VERIFY 1 $cfgmem
        set_property PROGRAM.CHECKSUM 0 $cfgmem

        create_hw_bitstream -hw_device $device [get_property PROGRAM.HW_CFGMEM_BITFILE $device]
        program_hw_devices $device
        refresh_hw_device $device
        program_hw_cfgmem -hw_cfgmem $cfgmem

        set_property PROBES.FILE {} $device
        set_property FULL_PROBES.FILE {} $device
        set_property PROGRAM.FILE $probe_bit_file_path $device
        program_hw_devices $device
        puts "STAGED $mcs_file_path"
    }
    default {
        error "unknown action: $action"
    }
}

close_hw_manager
