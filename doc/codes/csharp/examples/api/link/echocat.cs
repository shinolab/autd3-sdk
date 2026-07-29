using System;
using AUTD3;
using AUTD3.Link;

namespace DocSamples.ApiLinkEchocat;

internal static class Sample
{
    internal static void Run()
    {
        var iface = Interface.Auto;
        var sync0Period = TimeSpan.FromMilliseconds(1);
        var pduTimeout = TimeSpan.FromMilliseconds(100);
        var stateTransitionTimeout = TimeSpan.FromSeconds(10);
        uint dcStaticSyncIterations = 10000;
        var dcStartDelay = TimeSpan.FromMilliseconds(100);
        var dcSyncTolerance = TimeSpan.FromMicroseconds(1);
        var dcSyncTimeout = TimeSpan.FromSeconds(10);
        var processDataWatchdog = TimeSpan.FromMilliseconds(100);
        TimeSpan? spinMargin = null;
        // ANCHOR: api
        new EchocatLinkOption(
            iface,
            sync0Period,
            pduTimeout,
            stateTransitionTimeout,
            dcStaticSyncIterations,
            dcStartDelay,
            dcSyncTolerance,
            dcSyncTimeout,
            processDataWatchdog,
            spinMargin
        );
        // ANCHOR_END: api
    }
}
