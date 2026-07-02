using System;
using AUTD3.Link;

namespace AUTD3.DocSamples.ApiLinkEthercrab;

internal static class Sample
{
    internal static void Run()
    {
        var sync0Period = TimeSpan.FromMilliseconds(1);
        var sync0Shift = TimeSpan.FromMilliseconds(0);
        var syncTolerance = TimeSpan.FromMicroseconds(1);
        var syncTimeout = TimeSpan.FromSeconds(10);
        // ANCHOR: api
        new EtherCrabLinkOption(
            sync0Period: sync0Period,
            sync0Shift: sync0Shift,
            syncTolerance: syncTolerance,
            syncTimeout: syncTimeout
        );
        // ANCHOR_END: api
    }
}
