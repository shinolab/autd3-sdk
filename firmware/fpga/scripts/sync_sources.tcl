proc add_missing_files {fileset patterns} {
    set have [dict create]
    foreach f [get_files -quiet -of_objects [get_filesets $fileset]] {
        dict set have [file normalize $f] 1
    }
    foreach pattern $patterns {
        foreach f [glob -nocomplain $pattern] {
            set f [file normalize $f]
            if {[dict exists $have $f]} {
                continue
            }
            add_files -norecurse -fileset [get_filesets $fileset] $f
            set obj [get_files -of_objects [get_filesets $fileset] $f]
            set_property file_type SystemVerilog $obj
            set_property library xil_defaultlib $obj
            puts "added to $fileset: $f"
        }
    }
}

proc sync_project_sources {project_directory} {
    add_missing_files sources_1 [list \
        [file join $project_directory rtl/sources_1/new/*.sv] \
        [file join $project_directory rtl/sources_1/new/*/*.sv] \
        [file join $project_directory rtl/sources_1/new/*/*/*.sv]]
    add_missing_files sim_1 [list \
        [file join $project_directory rtl/sim_1/new/*.sv] \
        [file join $project_directory rtl/sim_1/new/*/*.sv]]
}
