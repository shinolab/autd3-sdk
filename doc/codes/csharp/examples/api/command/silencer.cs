using System;
using AUTD3;

namespace DocSamples.ApiCommandSilencer;

internal static class Sample
{
    internal static void Run()
    {
        var intensity = TimeSpan.FromMicroseconds(250);
        var phase = TimeSpan.FromMicroseconds(1000);
        var strictMode = true;
        // ANCHOR: api
        new SetSilencer();

        SetSilencer.Disable();

        new SetSilencer(new FixedCompletionTime
        {
            Intensity = intensity,
            Phase = phase,
            StrictMode = strictMode,
        });
        // ANCHOR_END: api

        ushort intensityRate = 256;
        ushort phaseRate = 256;

        // ANCHOR: api
        new SetSilencer(new FixedUpdateRate(intensityRate, phaseRate));
        // ANCHOR_END: api
    }
}
