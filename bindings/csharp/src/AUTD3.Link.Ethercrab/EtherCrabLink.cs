using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct EtherCrabLinkOption : ILink, ILegacyLink
    {
        public Interface Iface { get; }
        public TimeSpan? Sync0Period { get; }
        public TimeSpan? Sync0Shift { get; }
        public TimeSpan? SyncTolerance { get; }
        public TimeSpan? SyncTimeout { get; }
        public TimeSpan? PduTimeout { get; }
        public TimeSpan? StateTransitionTimeout { get; }
        public byte? TxRxPriority { get; }
        public bool DisableTxRxPriority { get; }
        public RtSchedulePolicy? TxRxPolicy { get; }
        public ulong? TxRxAffinity { get; }

        public EtherCrabLinkOption(
            Interface? iface = null,
            TimeSpan? sync0Period = null,
            TimeSpan? sync0Shift = null,
            TimeSpan? syncTolerance = null,
            TimeSpan? syncTimeout = null,
            TimeSpan? pduTimeout = null,
            TimeSpan? stateTransitionTimeout = null,
            byte? txRxPriority = null,
            bool disableTxRxPriority = false,
            RtSchedulePolicy? txRxPolicy = null,
            ulong? txRxAffinity = null)
        {
            if (txRxPriority.HasValue && disableTxRxPriority)
            {
                throw new ArgumentException("txRxPriority and disableTxRxPriority are mutually exclusive");
            }
            Iface = iface ?? Interface.Auto;
            Sync0Period = sync0Period;
            Sync0Shift = sync0Shift;
            SyncTolerance = syncTolerance;
            SyncTimeout = syncTimeout;
            PduTimeout = pduTimeout;
            StateTransitionTimeout = stateTransitionTimeout;
            TxRxPriority = txRxPriority;
            DisableTxRxPriority = disableTxRxPriority;
            TxRxPolicy = txRxPolicy;
            TxRxAffinity = txRxAffinity;
        }

        public static EtherCrabLinkOption SafeDefault(Interface? iface = null) =>
            FromPreset(iface, NativeEthercrab.autd3_link_ethercrab_option_safe_default);

        public static EtherCrabLinkOption PerformanceDefault(Interface? iface = null) =>
            FromPreset(iface, NativeEthercrab.autd3_link_ethercrab_option_performance_default);

        private delegate IntPtr PresetFn();

        private static EtherCrabLinkOption FromPreset(Interface? iface, PresetFn preset)
        {
            var handle = preset();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to get ethercrab link option preset");
            }
            try
            {
                LinkOptionNative.Apply("preset", NativeEthercrab.autd3_link_ethercrab_option_get_tx_rx_policy(handle, out var policy));
                return new EtherCrabLinkOption(
                    iface,
                    LinkOptionNative.GetDuration(handle, NativeEthercrab.autd3_link_ethercrab_option_get_sync0_period),
                    LinkOptionNative.GetDuration(handle, NativeEthercrab.autd3_link_ethercrab_option_get_sync0_shift),
                    LinkOptionNative.GetDuration(handle, NativeEthercrab.autd3_link_ethercrab_option_get_sync_tolerance),
                    LinkOptionNative.GetDuration(handle, NativeEthercrab.autd3_link_ethercrab_option_get_sync_timeout),
                    LinkOptionNative.GetDuration(handle, NativeEthercrab.autd3_link_ethercrab_option_get_pdu_timeout),
                    LinkOptionNative.GetDuration(handle, NativeEthercrab.autd3_link_ethercrab_option_get_state_transition_timeout),
                    txRxPolicy: (RtSchedulePolicy)policy);
            }
            finally
            {
                NativeEthercrab.autd3_link_ethercrab_option_free(handle);
            }
        }

        private IntPtr CreateHandle()
        {
            var handle = NativeEthercrab.autd3_link_ethercrab_option_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create ethercrab link option");
            }
            try
            {
                LinkOptionNative.Apply("iface", NativeEthercrab.autd3_link_ethercrab_option_set_iface(handle, Iface.NameValue));
                LinkOptionNative.SetDuration(handle, "sync0Period", Sync0Period, NativeEthercrab.autd3_link_ethercrab_option_set_sync0_period);
                LinkOptionNative.SetDuration(handle, "sync0Shift", Sync0Shift, NativeEthercrab.autd3_link_ethercrab_option_set_sync0_shift);
                LinkOptionNative.SetDuration(handle, "syncTolerance", SyncTolerance, NativeEthercrab.autd3_link_ethercrab_option_set_sync_tolerance);
                LinkOptionNative.SetDuration(handle, "syncTimeout", SyncTimeout, NativeEthercrab.autd3_link_ethercrab_option_set_sync_timeout);
                LinkOptionNative.SetDuration(handle, "pduTimeout", PduTimeout, NativeEthercrab.autd3_link_ethercrab_option_set_pdu_timeout);
                LinkOptionNative.SetDuration(handle, "stateTransitionTimeout", StateTransitionTimeout, NativeEthercrab.autd3_link_ethercrab_option_set_state_transition_timeout);
                if (TxRxPriority.HasValue || DisableTxRxPriority)
                {
                    LinkOptionNative.Apply("txRxPriority", NativeEthercrab.autd3_link_ethercrab_option_set_tx_rx_priority(
                        handle,
                        TxRxPriority.HasValue ? TxRxPriorityModeExplicit : TxRxPriorityModeDisabled,
                        TxRxPriority ?? 0));
                }
                if (TxRxPolicy is { } policy)
                {
                    LinkOptionNative.Apply("txRxPolicy", NativeEthercrab.autd3_link_ethercrab_option_set_tx_rx_policy(handle, (byte)policy));
                }
                if (TxRxAffinity.HasValue)
                {
                    LinkOptionNative.Apply("txRxAffinity", NativeEthercrab.autd3_link_ethercrab_option_set_tx_rx_affinity(handle, true, (UIntPtr)TxRxAffinity.Value));
                }
            }
            catch
            {
                NativeEthercrab.autd3_link_ethercrab_option_free(handle);
                throw;
            }
            return handle;
        }

        private const byte TxRxPriorityModeDisabled = 1;
        private const byte TxRxPriorityModeExplicit = 2;

        IntPtr ILink.TakeOpener() =>
            LinkOptionNative.TakeOpener("ethercrab", CreateHandle(), NativeEthercrab.autd3_link_ethercrab_open);

        IntPtr ILegacyLink.TakeLegacyOpener() =>
            LinkOptionNative.TakeOpener("ethercrab", CreateHandle(), NativeEthercrab.autd3_link_ethercrab_open_legacy);
    }

    internal static class NativeEthercrab
    {
        private const string Lib = "autd3_link_ethercrab";

        static NativeEthercrab() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_ethercrab_option_new();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_ethercrab_option_safe_default();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_ethercrab_option_performance_default();

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_iface(IntPtr option, [MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_sync0_period(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_get_sync0_period(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_sync0_shift(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_get_sync0_shift(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_sync_tolerance(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_get_sync_tolerance(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_sync_timeout(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_get_sync_timeout(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_pdu_timeout(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_get_pdu_timeout(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_state_transition_timeout(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_get_state_transition_timeout(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_tx_rx_priority(IntPtr option, byte mode, byte value);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_tx_rx_policy(IntPtr option, byte value);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_get_tx_rx_policy(IntPtr option, out byte value);

        [DllImport(Lib)]
        internal static extern int autd3_link_ethercrab_option_set_tx_rx_affinity(IntPtr option, [MarshalAs(UnmanagedType.I1)] bool hasAffinity, UIntPtr coreId);

        [DllImport(Lib)]
        internal static extern void autd3_link_ethercrab_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_ethercrab_open(IntPtr option, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_ethercrab_open_legacy(IntPtr option, byte[] outErr, UIntPtr outErrLen);
    }
}
