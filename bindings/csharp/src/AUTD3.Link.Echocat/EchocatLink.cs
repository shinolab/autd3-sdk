using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct EchocatLinkOption : ILink, ILegacyLink
    {
        public Interface Iface { get; }
        public TimeSpan? Sync0Period { get; }
        public TimeSpan? PduTimeout { get; }
        public TimeSpan? StateTransitionTimeout { get; }
        public uint? DcStaticSyncIterations { get; }
        public TimeSpan? DcStartDelay { get; }
        public TimeSpan? SyncTolerance { get; }
        public TimeSpan? SyncTimeout { get; }
        public TimeSpan? ProcessDataWatchdog { get; }
        public TimeSpan? SpinMargin { get; }

        public EchocatLinkOption(Interface? iface = null, TimeSpan? sync0Period = null, TimeSpan? pduTimeout = null, TimeSpan? stateTransitionTimeout = null, uint? dcStaticSyncIterations = null, TimeSpan? dcStartDelay = null, TimeSpan? syncTolerance = null, TimeSpan? syncTimeout = null, TimeSpan? processDataWatchdog = null, TimeSpan? spinMargin = null)
        {
            Iface = iface ?? Interface.Auto;
            Sync0Period = sync0Period;
            PduTimeout = pduTimeout;
            StateTransitionTimeout = stateTransitionTimeout;
            DcStaticSyncIterations = dcStaticSyncIterations;
            DcStartDelay = dcStartDelay;
            SyncTolerance = syncTolerance;
            SyncTimeout = syncTimeout;
            ProcessDataWatchdog = processDataWatchdog;
            SpinMargin = spinMargin;
        }

        private static ulong ToNs(TimeSpan? value) => (ulong)(value?.Ticks * 100 ?? 0);

        IntPtr ILink.TakeOpener()
        {
            var opener = NativeEchocat.autd3_link_echocat(
                Iface.NameValue,
                Sync0Period.HasValue, ToNs(Sync0Period),
                PduTimeout.HasValue, ToNs(PduTimeout),
                StateTransitionTimeout.HasValue, ToNs(StateTransitionTimeout),
                DcStaticSyncIterations.HasValue, DcStaticSyncIterations ?? 0,
                DcStartDelay.HasValue, ToNs(DcStartDelay),
                SyncTolerance.HasValue, ToNs(SyncTolerance),
                SyncTimeout.HasValue, ToNs(SyncTimeout),
                ProcessDataWatchdog.HasValue, ToNs(ProcessDataWatchdog),
                SpinMargin.HasValue, ToNs(SpinMargin));
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create echocat link");
            }
            return opener;
        }

        IntPtr ILegacyLink.TakeLegacyOpener()
        {
            var opener = NativeEchocat.autd3_link_echocat_legacy(
                Iface.NameValue,
                Sync0Period.HasValue, ToNs(Sync0Period),
                PduTimeout.HasValue, ToNs(PduTimeout),
                StateTransitionTimeout.HasValue, ToNs(StateTransitionTimeout),
                DcStaticSyncIterations.HasValue, DcStaticSyncIterations ?? 0,
                DcStartDelay.HasValue, ToNs(DcStartDelay),
                SyncTolerance.HasValue, ToNs(SyncTolerance),
                SyncTimeout.HasValue, ToNs(SyncTimeout),
                ProcessDataWatchdog.HasValue, ToNs(ProcessDataWatchdog),
                SpinMargin.HasValue, ToNs(SpinMargin));
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create echocat link");
            }
            return opener;
        }
    }

    internal static class NativeEchocat
    {
        private const string Lib = "autd3_link_echocat";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_echocat(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName,
            [MarshalAs(UnmanagedType.I1)] bool hasSync0Period, ulong sync0PeriodNs,
            [MarshalAs(UnmanagedType.I1)] bool hasPduTimeout, ulong pduTimeoutNs,
            [MarshalAs(UnmanagedType.I1)] bool hasStateTransitionTimeout, ulong stateTransitionTimeoutNs,
            [MarshalAs(UnmanagedType.I1)] bool hasDcStaticSyncIterations, uint dcStaticSyncIterations,
            [MarshalAs(UnmanagedType.I1)] bool hasDcStartDelay, ulong dcStartDelayNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTolerance, ulong syncToleranceNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTimeout, ulong syncTimeoutNs,
            [MarshalAs(UnmanagedType.I1)] bool hasProcessDataWatchdog, ulong processDataWatchdogNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSpinMargin, ulong spinMarginNs);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_echocat_legacy(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName,
            [MarshalAs(UnmanagedType.I1)] bool hasSync0Period, ulong sync0PeriodNs,
            [MarshalAs(UnmanagedType.I1)] bool hasPduTimeout, ulong pduTimeoutNs,
            [MarshalAs(UnmanagedType.I1)] bool hasStateTransitionTimeout, ulong stateTransitionTimeoutNs,
            [MarshalAs(UnmanagedType.I1)] bool hasDcStaticSyncIterations, uint dcStaticSyncIterations,
            [MarshalAs(UnmanagedType.I1)] bool hasDcStartDelay, ulong dcStartDelayNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTolerance, ulong syncToleranceNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTimeout, ulong syncTimeoutNs,
            [MarshalAs(UnmanagedType.I1)] bool hasProcessDataWatchdog, ulong processDataWatchdogNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSpinMargin, ulong spinMarginNs);
    }
}
