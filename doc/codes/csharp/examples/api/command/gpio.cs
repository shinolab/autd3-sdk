using AUTD3;

namespace DocSamples.ApiCommandGpio;

internal static class Sample
{
    internal static void Run()
    {
        var gpio0 = GpioOut.PatternBank;
        var gpio1 = GpioOut.Thermo;
        var gpio2 = GpioOut.PwmOut(0);
        var gpio3 = GpioOut.Off;
        var outputs = new[] { gpio0, gpio1, gpio2, gpio3 };
        // ANCHOR: api
        new SetGpioOut(outputs);
        // ANCHOR_END: api
    }
}
