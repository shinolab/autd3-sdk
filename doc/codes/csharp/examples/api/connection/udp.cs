using System;
using AUTD3;

namespace DocSamples.ApiConnectionUdp;

internal static class Sample
{
    internal static void Run()
    {
        var iface = Interface.Auto;
        var cycle = TimeSpan.FromMilliseconds(1);
        var replyTimeout = TimeSpan.FromMilliseconds(1);
        var responseTimeout = TimeSpan.FromMilliseconds(200);
        var enumerationTimeout = TimeSpan.FromSeconds(10);
        var syncTimeout = TimeSpan.FromSeconds(5);
        // ANCHOR: api
        new TransportOption
        {
            Iface = iface,
            Cycle = cycle,
            ReplyTimeout = replyTimeout,
            ResponseTimeout = responseTimeout,
            EnumerationTimeout = enumerationTimeout,
            SyncTimeout = syncTimeout,
        };
        // ANCHOR_END: api
    }
}
