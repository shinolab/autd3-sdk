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
        var framePhase = FramePhase.Auto;
        var pduTimeout = TimeSpan.FromMilliseconds(100);
        var stateTransitionTimeout = TimeSpan.FromSeconds(10);
        uint dcStaticSyncIterations = 10000;
        var dcStartDelay = TimeSpan.FromMilliseconds(100);
        var syncTolerance = TimeSpan.FromMicroseconds(1);
        var syncTimeout = TimeSpan.FromSeconds(10);
        var processDataWatchdog = TimeSpan.FromMilliseconds(100);
        // ANCHOR: api
        new EchocatLinkOption
        {
            Iface = iface,
            Sync0Period = sync0Period,
            FramePhase = framePhase,
            PduTimeout = pduTimeout,
            StateTransitionTimeout = stateTransitionTimeout,
            DcStaticSyncIterations = dcStaticSyncIterations,
            DcStartDelay = dcStartDelay,
            SyncTolerance = syncTolerance,
            SyncTimeout = syncTimeout,
            ProcessDataWatchdog = processDataWatchdog,
        };
        // ANCHOR_END: api

        // ANCHOR: frame_phase
        _ = FramePhase.Auto;
        _ = FramePhase.At(TimeSpan.FromMicroseconds(500));
        // ANCHOR_END: frame_phase
    }
}
