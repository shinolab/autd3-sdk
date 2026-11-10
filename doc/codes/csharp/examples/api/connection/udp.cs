using System;
using AUTD3;

namespace DocSamples.ApiConnectionUdp;

internal static class Sample
{
    internal static void Run()
    {
        var iface = Interface.Auto;
        ulong sendBufferBytes = 8192;
        var syncOnOpen = true;
        TimeSpan? heartbeatInterval = null;
        // ANCHOR: api
        new TransportOption
        {
            Iface = iface,
            SendBufferBytes = sendBufferBytes,
            SyncOnOpen = syncOnOpen,
            HeartbeatInterval = heartbeatInterval,
        };
        // ANCHOR_END: api

        // ANCHOR: iface
        _ = Interface.Auto;
        _ = Interface.Name("eth0");
        _ = Interface.Addr("[::1]:44336");
        // ANCHOR_END: iface
    }
}
