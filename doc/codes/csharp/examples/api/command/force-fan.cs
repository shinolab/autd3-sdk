using AUTD3;

namespace DocSamples.ApiCommandForceFan;

internal static class Sample
{
    internal static void Run()
    {
        var value = true;
        // ANCHOR: api
        new ForceFan(value);
        // ANCHOR_END: api
    }
}
