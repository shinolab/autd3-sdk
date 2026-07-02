using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct EtherCrabLinkOption
    {
        public TimeSpan? Sync0Period { get; }
        public TimeSpan? Sync0Shift { get; }
        public TimeSpan? SyncTolerance { get; }
        public TimeSpan? SyncTimeout { get; }

        public EtherCrabLinkOption(TimeSpan? sync0Period = null, TimeSpan? sync0Shift = null, TimeSpan? syncTolerance = null, TimeSpan? syncTimeout = null)
        {
            Sync0Period = sync0Period;
            Sync0Shift = sync0Shift;
            SyncTolerance = syncTolerance;
            SyncTimeout = syncTimeout;
        }
    }

    public sealed class EtherCrabLink : ILink
    {
        private IntPtr _opener;

        private EtherCrabLink(IntPtr opener)
        {
            _opener = opener;
        }

        public static EtherCrabLink Create(string? interfaceName = null, EtherCrabLinkOption option = default)
        {
            var opener = NativeEthercrab.autd3_link_ethercrab(
                interfaceName,
                option.Sync0Period.HasValue, (ulong)(option.Sync0Period?.Ticks * 100 ?? 0),
                option.Sync0Shift.HasValue, (ulong)(option.Sync0Shift?.Ticks * 100 ?? 0),
                option.SyncTolerance.HasValue, (ulong)(option.SyncTolerance?.Ticks * 100 ?? 0),
                option.SyncTimeout.HasValue, (ulong)(option.SyncTimeout?.Ticks * 100 ?? 0));
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create ethercrab link");
            }
            return new EtherCrabLink(opener);
        }

        public IntPtr TakeOpener()
        {
            var opener = _opener;
            _opener = IntPtr.Zero;
            return opener;
        }
    }

    internal static class NativeEthercrab
    {
        private const string Lib = "autd3_link_ethercrab";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_ethercrab(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName,
            [MarshalAs(UnmanagedType.I1)] bool hasSync0Period, ulong sync0PeriodNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSync0Shift, ulong sync0ShiftNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTolerance, ulong syncToleranceNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTimeout, ulong syncTimeoutNs);
    }
}
