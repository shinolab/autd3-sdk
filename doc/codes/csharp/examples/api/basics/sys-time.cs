using System;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiBasicsSysTime;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });
        using var client = Client.Open(geometry, new TransportOption());

        // ANCHOR: construct
        var opened = SysTime.Zero;
        var now = client.DeviceTimeNow();
        var raw = SysTime.FromNanos(1_000_000_000);
        ulong ns = now.Nanos;
        // ANCHOR_END: construct

        // ANCHOR: ops
        var future = client.DeviceTimeNow() + TimeSpan.FromMilliseconds(100);
        var past = future - TimeSpan.FromMilliseconds(50);
        TimeSpan elapsed = future - past;
        bool isAfter = future > past; // true
        // ANCHOR_END: ops

        _ = (opened, raw, ns, isAfter);
    }
}
