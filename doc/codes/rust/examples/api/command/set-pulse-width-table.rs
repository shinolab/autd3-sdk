use autd3_rs::commands::SetPulseWidthTable;

fn main() {
    let table = SetPulseWidthTable::default_table();

    // ANCHOR: api
    SetPulseWidthTable { table: &table };
    // ANCHOR_END: api
}
