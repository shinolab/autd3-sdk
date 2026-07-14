using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct EtherCrabLinkOption : ILink
    {
        public Interface Iface { get; }
        public TimeSpan? Sync0Period { get; }
        public TimeSpan? Sync0Shift { get; }
        public TimeSpan? SyncTolerance { get; }
        public TimeSpan? SyncTimeout { get; }

        public EtherCrabLinkOption(Interface? iface = null, TimeSpan? sync0Period = null, TimeSpan? sync0Shift = null, TimeSpan? syncTolerance = null, TimeSpan? syncTimeout = null)
        {
            Iface = iface ?? Interface.Auto;
            Sync0Period = sync0Period;
            Sync0Shift = sync0Shift;
            SyncTolerance = syncTolerance;
            SyncTimeout = syncTimeout;
        }

        public static EtherCrabLinkOption SafeDefault(Interface? iface = null) =>
            FromValues(iface, NativeEthercrab.autd3_link_ethercrab_option_safe_default);

        public static EtherCrabLinkOption PerformanceDefault(Interface? iface = null) =>
            FromValues(iface, NativeEthercrab.autd3_link_ethercrab_option_performance_default);

        private delegate int PresetFn(out EtherCrabLinkOptionValues values);

        private static EtherCrabLinkOption FromValues(Interface? iface, PresetFn preset)
        {
            if (preset(out var values) != 0)
            {
                throw new Autd3Exception("failed to get ethercrab link option preset");
            }
            return new EtherCrabLinkOption(
                iface,
                ToTimeSpan(values.Sync0PeriodNs),
                ToTimeSpan(values.Sync0ShiftNs),
                ToTimeSpan(values.SyncToleranceNs),
                ToTimeSpan(values.SyncTimeoutNs));
        }

        private static TimeSpan ToTimeSpan(ulong ns) => TimeSpan.FromTicks((long)(ns / 100));

        IntPtr ILink.TakeOpener()
        {
            var opener = NativeEthercrab.autd3_link_ethercrab(
                Iface.NameValue,
                Sync0Period.HasValue, (ulong)(Sync0Period?.Ticks * 100 ?? 0),
                Sync0Shift.HasValue, (ulong)(Sync0Shift?.Ticks * 100 ?? 0),
                SyncTolerance.HasValue, (ulong)(SyncTolerance?.Ticks * 100 ?? 0),
                SyncTimeout.HasValue, (ulong)(SyncTimeout?.Ticks * 100 ?? 0));
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create ethercrab link");
            }
            return opener;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct EtherCrabLinkOptionValues
    {
        public ulong Sync0PeriodNs;
        public ulong Sync0ShiftNs;
        public ulong SyncToleranceNs;
        public ulong SyncTimeoutNs;
    }

    internal static class NativeEthercrab
    {
        private const string Lib = "autd3_link_ethercrab";

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_safe_default(out EtherCrabLinkOptionValues @out);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_performance_default(out EtherCrabLinkOptionValues @out);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_ethercrab(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName,
            [MarshalAs(UnmanagedType.I1)] bool hasSync0Period, ulong sync0PeriodNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSync0Shift, ulong sync0ShiftNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTolerance, ulong syncToleranceNs,
            [MarshalAs(UnmanagedType.I1)] bool hasSyncTimeout, ulong syncTimeoutNs);
    }
}
