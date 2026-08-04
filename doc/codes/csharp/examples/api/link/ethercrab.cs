using System;
using AUTD3;
using AUTD3.Link;

namespace DocSamples.ApiLinkEthercrab;

internal static class Sample
{
    internal static void Run()
    {
        var iface = Interface.Auto;
        var sync0Period = TimeSpan.FromMilliseconds(1);
        var sync0Shift = TimeSpan.FromMilliseconds(0);
        var syncTolerance = TimeSpan.FromMicroseconds(1);
        var syncTimeout = TimeSpan.FromSeconds(10);
        // ANCHOR: api
        new EtherCrabLinkOption(
            iface,
            sync0Period,
            sync0Shift,
            syncTolerance,
            syncTimeout
        );
        // ANCHOR_END: api

        var pduTimeout = TimeSpan.FromMilliseconds(30);
        var stateTransitionTimeout = TimeSpan.FromSeconds(10);
        var txRxAffinity = 0UL;
        // ANCHOR: api_extra
        new EtherCrabLinkOption(
            iface,
            sync0Period,
            sync0Shift,
            syncTolerance,
            syncTimeout,
            pduTimeout,
            stateTransitionTimeout,
            txRxPriority: 90,
            disableTxRxPriority: false,
            txRxPolicy: RtSchedulePolicy.Fifo,
            txRxAffinity: txRxAffinity
        );
        // ANCHOR_END: api_extra
    }
}
