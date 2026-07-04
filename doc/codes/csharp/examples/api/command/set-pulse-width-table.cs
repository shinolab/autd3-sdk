using AUTD3;

namespace DocSamples.ApiCommandSetPulseWidthTable;

internal static class Sample
{
    internal static void Run()
    {
        var table = SetPulseWidthTable.DefaultTable();

        // ANCHOR: api
        new SetPulseWidthTable(table);
        // ANCHOR_END: api
    }
}
