using System;
using AUTD3;

namespace DocSamples.ApiConnectionUdp;

internal static class Sample
{
    internal static void Run()
    {
        var iface = Interface.Auto;
        var recoveryReplyTimeout = TimeSpan.FromMilliseconds(1);
        var lostTimeout = TimeSpan.FromMilliseconds(100);
        var controlRequestTimeout = TimeSpan.FromMilliseconds(200);
        var enumerationTimeout = TimeSpan.FromSeconds(10);
        var syncTimeout = TimeSpan.FromSeconds(30);
        var ackTimeout = TimeSpan.FromMilliseconds(10);
        float? sendRateLimitPercent = null;
        ulong sendBufferBytes = 8192;
        // ANCHOR: api
        new TransportOption
        {
            Iface = iface,
            RecoveryReplyTimeout = recoveryReplyTimeout,
            LostTimeout = lostTimeout,
            ControlRequestTimeout = controlRequestTimeout,
            EnumerationTimeout = enumerationTimeout,
            SyncTimeout = syncTimeout,
            AckTimeout = ackTimeout,
            SendRateLimitPercent = sendRateLimitPercent,
            SendBufferBytes = sendBufferBytes,
        };
        // ANCHOR_END: api

        // ANCHOR: iface
        _ = Interface.Auto;
        _ = Interface.Name("eth0");
        _ = Interface.Simulator(TransportOption.SimulatorPort);
        _ = Interface.Addr("[::1]:44336");
        // ANCHOR_END: iface
    }
}
