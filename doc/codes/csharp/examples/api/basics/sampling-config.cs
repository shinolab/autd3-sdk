using System;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiBasicsSamplingConfig;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        new SamplingConfig(10);
        new SamplingConfig(4.0f * kHz);
        new SamplingConfig(TimeSpan.FromMicroseconds(250));
        new SamplingConfig(Nearest(4.0f * kHz));
        new SamplingConfig(Nearest(TimeSpan.FromMicroseconds(250)));
        // ANCHOR_END: api
    }
}
