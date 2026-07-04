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

        new SetSilencer(new FixedCompletionTime(
            intensity: intensity,
            phase: phase,
            strictMode: strictMode
        ));
        // ANCHOR_END: api

        ushort intensityRate = 256;
        ushort phaseRate = 256;

        // ANCHOR: api
        new SetSilencer(new FixedUpdateRate(intensity: intensityRate, phase: phaseRate));
        // ANCHOR_END: api
    }
}
