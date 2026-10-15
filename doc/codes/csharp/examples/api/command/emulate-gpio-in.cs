using AUTD3;

namespace DocSamples.ApiCommandEmulateGpioIn;

internal static class Sample
{
    internal static void Run()
    {
        var values = new[] { true, false, false, false };
        // ANCHOR: api
        new EmulateGpioIn(values);
        // ANCHOR_END: api
    }
}
