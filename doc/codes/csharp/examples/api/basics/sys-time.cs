using System;
using AUTD3;

namespace DocSamples.ApiBasicsSysTime;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: construct
        var epoch = SysTime.Zero; 
        var now = SysTime.Now();
        var at = SysTime.FromUtc(new DateTime(2025, 1, 1, 0, 0, 0, DateTimeKind.Utc));
        var raw = SysTime.FromNanos(1_000_000_000);
        ulong ns = now.Nanos;
        DateTime utc = now.ToUtc();
        // ANCHOR_END: construct

        // ANCHOR: ops
        var future = SysTime.Now() + TimeSpan.FromMilliseconds(100);
        var past = future - TimeSpan.FromMilliseconds(50);
        bool isAfter = future > past; // true
        // ANCHOR_END: ops

        _ = (epoch, at, raw, ns, utc, isAfter);
    }
}
