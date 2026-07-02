using AUTD3;

namespace AUTD3.DocSamples.ApiCommandSetPulseWidthTable;

internal static class Sample
{
    internal static void Run()
    {
        var table = PulseWidth.DefaultTable();

        // ANCHOR: api
        new SetPulseWidthTable(table);
        // ANCHOR_END: api
    }
}
