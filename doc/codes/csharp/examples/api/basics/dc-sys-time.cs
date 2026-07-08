using System;
using AUTD3;

namespace DocSamples.ApiBasicsDcSysTime;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: construct
        var epoch = DcSysTime.Zero; 
        var now = DcSysTime.Now();
        var at = DcSysTime.FromUtc(new DateTime(2025, 1, 1, 0, 0, 0, DateTimeKind.Utc));
        var raw = DcSysTime.FromNanos(1_000_000_000);
        ulong ns = now.SysTime;
        DateTime utc = now.ToUtc();
        // ANCHOR_END: construct

        // ANCHOR: ops
        var future = DcSysTime.Now() + TimeSpan.FromMilliseconds(100);
        var past = future - TimeSpan.FromMilliseconds(50);
        bool isAfter = future > past; // true
        // ANCHOR_END: ops

        _ = (epoch, at, raw, ns, utc, isAfter);
    }
}
