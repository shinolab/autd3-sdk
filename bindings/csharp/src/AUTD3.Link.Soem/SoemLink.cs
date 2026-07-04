using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct SoemLinkOption : ILink
    {
        public Interface Iface { get; }
        public TimeSpan? Sync0Period { get; }
        public TimeSpan? Sync0Shift { get; }
        public TimeSpan? SyncTolerance { get; }
        public TimeSpan? SyncTimeout { get; }

        public SoemLinkOption(Interface? iface = null, TimeSpan? sync0Period = null, TimeSpan? sync0Shift = null, TimeSpan? syncTolerance = null, TimeSpan? syncTimeout = null)
        {
            Iface = iface ?? Interface.Auto;
            Sync0Period = sync0Period;
            Sync0Shift = sync0Shift;
            SyncTolerance = syncTolerance;
            SyncTimeout = syncTimeout;
        }

        IntPtr ILink.TakeOpener()
        {
            var opener = NativeSoem.autd3_link_soem(
                Iface.NameValue,
                Sync0Period.HasValue, (ulong)(Sync0Period?.Ticks * 100 ?? 0),
                Sync0Shift.HasValue, (ulong)(Sync0Shift?.Ticks * 100 ?? 0),
                SyncTolerance.HasValue, (ulong)(SyncTolerance?.Ticks * 100 ?? 0),
                SyncTimeout.HasValue, (ulong)(SyncTimeout?.Ticks * 100 ?? 0));
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create soem link");
            }
            return opener;
        }
    }

    internal static class NativeSoem
    {
        private const string Lib = "autd3_link_soem";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_soem(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName,
            [MarshalAs(UnmanagedType.I1)] bool hasSync0Period, ulong sync0PeriodNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSync0Shift, ulong sync0ShiftNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTolerance, ulong syncToleranceNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTimeout, ulong syncTimeoutNs);
    }
}
