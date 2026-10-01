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

proc remove_stale_files {project_directory fileset} {
    set stale [list]
    foreach f [get_files -quiet -norecurse -of_objects [get_filesets $fileset]] {
        set path [get_property NAME $f]
        if {[string equal [file extension $path] ".xci"]} {
            set name [file rootname [file tail $path]]
            set present [file exists [file join $project_directory rtl/sources_1/ip $name "$name.xci"]]
        } else {
            set present [file exists $path]
        }
        if {!$present} {
            lappend stale $path
        }
    }
    foreach path $stale {
        remove_files -fileset [get_filesets $fileset] $path
        puts "removed from $fileset: $path"
    }
}

proc sync_project_sources {project_directory} {
    remove_stale_files $project_directory sources_1
    remove_stale_files $project_directory sim_1
    add_missing_files sources_1 [list \
        [file join $project_directory rtl/sources_1/new/*.sv] \
        [file join $project_directory rtl/sources_1/new/*/*.sv] \
        [file join $project_directory rtl/sources_1/new/*/*/*.sv]]
    add_missing_files sim_1 [list \
        [file join $project_directory rtl/sim_1/new/*.sv] \
        [file join $project_directory rtl/sim_1/new/*/*.sv]]
}
