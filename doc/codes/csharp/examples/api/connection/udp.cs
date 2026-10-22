using System;
using AUTD3;

namespace DocSamples.ApiConnectionUdp;

internal static class Sample
{
    internal static void Run()
    {
        var iface = Interface.Auto;
        var openTimeout = TimeSpan.FromSeconds(40);
        var recoveryReplyTimeout = TimeSpan.FromMilliseconds(1);
        var lostTimeout = TimeSpan.FromMilliseconds(100);
        var ackTimeout = TimeSpan.FromMilliseconds(10);
        ulong sendBufferBytes = 8192;
        // ANCHOR: api
        new TransportOption
        {
            Iface = iface,
            OpenTimeout = openTimeout,
            RecoveryReplyTimeout = recoveryReplyTimeout,
            LostTimeout = lostTimeout,
            AckTimeout = ackTimeout,
            SendBufferBytes = sendBufferBytes,
        };
        // ANCHOR_END: api

        // ANCHOR: iface
        _ = Interface.Auto;
        _ = Interface.Name("eth0");
        _ = Interface.Addr("[::1]:44336");
        // ANCHOR_END: iface
    }
}
