using System;
using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiBasicsSamplingConfig;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        SamplingConfig.Divide(10);
        SamplingConfig.FromFreq(4.0f * kHz);
        SamplingConfig.FromPeriod(TimeSpan.FromMicroseconds(250));
        SamplingConfig.FromFreq(Nearest(4.0f * kHz));
        SamplingConfig.FromPeriodNearest(TimeSpan.FromMicroseconds(250));
        // ANCHOR_END: api
    }
}
