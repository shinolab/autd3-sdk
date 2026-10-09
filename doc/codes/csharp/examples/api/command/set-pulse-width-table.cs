using AUTD3;

namespace DocSamples.ApiCommandSetPulseWidthTable;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: empty
        var table = SetPulseWidthTable.EmptyTable();
        // ANCHOR_END: empty

        // ANCHOR: api
        new SetPulseWidthTable(table);
        // ANCHOR_END: api

        // ANCHOR: default
        new SetPulseWidthTable();
        // ANCHOR_END: default
    }
}
