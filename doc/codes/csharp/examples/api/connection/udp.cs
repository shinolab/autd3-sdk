using System;
using AUTD3;

namespace DocSamples.ApiConnectionUdp;

internal static class Sample
{
    internal static void Run()
    {
        var iface = Interface.Auto;
        var heartbeat = TimeSpan.FromMilliseconds(10);
        var replyTimeout = TimeSpan.FromMilliseconds(1);
        var lostTimeout = TimeSpan.FromMilliseconds(100);
        var responseTimeout = TimeSpan.FromMilliseconds(200);
        var enumerationTimeout = TimeSpan.FromSeconds(10);
        var syncTimeout = TimeSpan.FromSeconds(30);
        float? sendRateLimit = null;
        uint sendBuffer = 8192;
        var timerResolution = TimeSpan.FromMilliseconds(1);
        // ANCHOR: api
        new TransportOption
        {
            Iface = iface,
            Heartbeat = heartbeat,
            ReplyTimeout = replyTimeout,
            LostTimeout = lostTimeout,
            ResponseTimeout = responseTimeout,
            EnumerationTimeout = enumerationTimeout,
            SyncTimeout = syncTimeout,
            SendRateLimit = sendRateLimit,
            SendBuffer = sendBuffer,
            TimerResolution = timerResolution,
        };
        // ANCHOR_END: api

        // ANCHOR: iface
        _ = Interface.Auto;
        _ = Interface.Name("eth0");
        _ = Interface.Simulator;
        // ANCHOR_END: iface
    }
}
