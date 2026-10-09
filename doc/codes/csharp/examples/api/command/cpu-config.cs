using System;
using AUTD3;

namespace DocSamples.ApiCommandCpuConfig;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        var config = new CpuConfig
        {
            SysTimeTransitionMargin = TimeSpan.FromMilliseconds(10),
            FpgaWaitUpdateMaxPolls = 1_000_000,
            FpgaFlashMaxPolls = 2_000_000_000,
            SyncGuard = TimeSpan.FromMicroseconds(250),
            UpdateActivateDelay = TimeSpan.FromMilliseconds(100),
            FailsafeTimeout = TimeSpan.FromMilliseconds(500),
            PtpUnlockFailsafeTimeout = null,
            FpgaBusWait = FpgaBusWait.Cycles3,
            Ptp = new PtpConfig
            {
                SyncInterval = TimeSpan.FromMilliseconds(8),
                TxTimestampTimeout = TimeSpan.FromMilliseconds(3),
                DelayRespTimeout = TimeSpan.FromMilliseconds(6),
                Holdover = TimeSpan.FromSeconds(1),
                LockSamples = 64,
                StepThreshold = TimeSpan.FromMicroseconds(10),
                LockThreshold = TimeSpan.FromTicks(1),
                KpMilli = 50,
                KiMilli = 1,
                MaxFreqPpb = 500_000,
                DelayReqSyncs = 32,
                PathDelayFilterShift = 5,
                PauseQuanta = 48,
                PauseHoldSyncs = 64,
                PauseRetry = TimeSpan.FromMilliseconds(8),
            },
        };
        new SetCpuConfig(config);
        // ANCHOR_END: api
    }
}
