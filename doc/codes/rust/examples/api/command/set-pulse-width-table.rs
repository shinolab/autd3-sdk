use autd3_rs::commands::SetPulseWidthTable;

fn main() {
    // ANCHOR: empty
    let table = SetPulseWidthTable::empty_table();
    // ANCHOR_END: empty

    // ANCHOR: api
    SetPulseWidthTable { table: &table };
    // ANCHOR_END: api

    // ANCHOR: default
    SetPulseWidthTable::default();
    // ANCHOR_END: default
}
