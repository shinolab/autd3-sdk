set bit_file_path [lindex $argv 0]
set mcs_file_path [lindex $argv 1]

write_cfgmem -format mcs -size 16 -interface SPIx4 -loadbit "up 0x00000000 $bit_file_path" -force -file $mcs_file_path
puts "MCS $mcs_file_path"
