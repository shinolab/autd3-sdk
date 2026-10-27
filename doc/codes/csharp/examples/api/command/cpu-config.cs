using System;
using AUTD3;

namespace DocSamples.ApiCommandCpuConfig;

internal static class Sample
{
    internal static void Run()
    {
        var syncGuard = TimeSpan.FromMicroseconds(250);
        TimeSpan? failsafeTimeout = TimeSpan.FromMilliseconds(500);
        TimeSpan? ptpUnlockFailsafeTimeout = null;
        var fpgaBusWait = FpgaBusWait.Cycles3;
        TimeSpan? cpuWatchdogTimeout = TimeSpan.FromSeconds(1);
        var syncInterval = TimeSpan.FromMilliseconds(8);
        var txTimestampTimeout = TimeSpan.FromMilliseconds(3);
        var delayRespTimeout = TimeSpan.FromMilliseconds(6);
        var holdover = TimeSpan.FromSeconds(1);
        ushort lockSamples = 64;
        var stepThreshold = TimeSpan.FromMicroseconds(10);
        var lockThreshold = TimeSpan.FromTicks(1);
        uint kpMilli = 50;
        uint kiMilli = 1;
        uint maxFreqPpb = 500_000;
        ushort delayReqSyncs = 32;
        byte pathDelayFilterShift = 5;
        ushort? pauseQuanta = 48;
        ushort pauseHoldSyncs = 64;
        var pauseRetry = TimeSpan.FromMilliseconds(8);
        // ANCHOR: api
        var config = new CpuConfig
        {
            SyncGuard = syncGuard,
            FailsafeTimeout = failsafeTimeout,
            PtpUnlockFailsafeTimeout = ptpUnlockFailsafeTimeout,
            FpgaBusWait = fpgaBusWait,
            CpuWatchdogTimeout = cpuWatchdogTimeout,
            Ptp = new PtpConfig
            {
                SyncInterval = syncInterval,
                TxTimestampTimeout = txTimestampTimeout,
                DelayRespTimeout = delayRespTimeout,
                Holdover = holdover,
                LockSamples = lockSamples,
                StepThreshold = stepThreshold,
                LockThreshold = lockThreshold,
                KpMilli = kpMilli,
                KiMilli = kiMilli,
                MaxFreqPpb = maxFreqPpb,
                DelayReqSyncs = delayReqSyncs,
                PathDelayFilterShift = pathDelayFilterShift,
                PauseQuanta = pauseQuanta,
                PauseHoldSyncs = pauseHoldSyncs,
                PauseRetry = pauseRetry,
            },
        };
        new SetCpuConfig(config);
        // ANCHOR_END: api
    }
}
