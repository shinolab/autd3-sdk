using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct FramePhase
    {
        internal TimeSpan? Phase { get; }

        private FramePhase(TimeSpan? phase)
        {
            Phase = phase;
        }

        public static FramePhase Auto => new FramePhase(null);

        public static FramePhase At(TimeSpan phase) => new FramePhase(phase);
    }

    public readonly struct EchocatLinkOption : ILink, ILegacyLink
    {
        public Interface Iface { get; }
        public TimeSpan? Sync0Period { get; }
        public FramePhase FramePhase { get; }
        public TimeSpan? PduTimeout { get; }
        public TimeSpan? StateTransitionTimeout { get; }
        public uint? DcStaticSyncIterations { get; }
        public TimeSpan? DcStartDelay { get; }
        public TimeSpan? SyncTolerance { get; }
        public TimeSpan? SyncTimeout { get; }
        public TimeSpan? ProcessDataWatchdog { get; }
        public TimeSpan? SpinMargin { get; }

        public EchocatLinkOption(Interface? iface = null, TimeSpan? sync0Period = null, FramePhase framePhase = default, TimeSpan? pduTimeout = null, TimeSpan? stateTransitionTimeout = null, uint? dcStaticSyncIterations = null, TimeSpan? dcStartDelay = null, TimeSpan? syncTolerance = null, TimeSpan? syncTimeout = null, TimeSpan? processDataWatchdog = null, TimeSpan? spinMargin = null)
        {
            Iface = iface ?? Interface.Auto;
            Sync0Period = sync0Period;
            FramePhase = framePhase;
            PduTimeout = pduTimeout;
            StateTransitionTimeout = stateTransitionTimeout;
            DcStaticSyncIterations = dcStaticSyncIterations;
            DcStartDelay = dcStartDelay;
            SyncTolerance = syncTolerance;
            SyncTimeout = syncTimeout;
            ProcessDataWatchdog = processDataWatchdog;
            SpinMargin = spinMargin;
        }

        private IntPtr CreateHandle()
        {
            var handle = NativeEchocat.autd3_link_echocat_option_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create echocat link option");
            }
            try
            {
                LinkOptionNative.Apply("iface", NativeEchocat.autd3_link_echocat_option_set_iface(handle, Iface.NameValue));
                LinkOptionNative.SetDuration(handle, "sync0Period", Sync0Period, NativeEchocat.autd3_link_echocat_option_set_sync0_period);
                var framePhase = FramePhase.Phase;
                LinkOptionNative.Apply("framePhase", NativeEchocat.autd3_link_echocat_option_set_frame_phase(handle, framePhase.HasValue, framePhase is { } phase ? LinkOptionNative.ToNanos(phase) : 0UL));
                LinkOptionNative.SetDuration(handle, "pduTimeout", PduTimeout, NativeEchocat.autd3_link_echocat_option_set_pdu_timeout);
                LinkOptionNative.SetDuration(handle, "stateTransitionTimeout", StateTransitionTimeout, NativeEchocat.autd3_link_echocat_option_set_state_transition_timeout);
                if (DcStaticSyncIterations is { } iterations)
                {
                    LinkOptionNative.Apply("dcStaticSyncIterations", NativeEchocat.autd3_link_echocat_option_set_dc_static_sync_iterations(handle, iterations));
                }
                LinkOptionNative.SetDuration(handle, "dcStartDelay", DcStartDelay, NativeEchocat.autd3_link_echocat_option_set_dc_start_delay);
                LinkOptionNative.SetDuration(handle, "syncTolerance", SyncTolerance, NativeEchocat.autd3_link_echocat_option_set_sync_tolerance);
                LinkOptionNative.SetDuration(handle, "syncTimeout", SyncTimeout, NativeEchocat.autd3_link_echocat_option_set_sync_timeout);
                LinkOptionNative.SetDuration(handle, "processDataWatchdog", ProcessDataWatchdog, NativeEchocat.autd3_link_echocat_option_set_process_data_watchdog);
                if (SpinMargin is { } margin)
                {
                    LinkOptionNative.Apply("spinMargin", NativeEchocat.autd3_link_echocat_option_set_spin_margin(handle, true, LinkOptionNative.ToNanos(margin)));
                }
            }
            catch
            {
                NativeEchocat.autd3_link_echocat_option_free(handle);
                throw;
            }
            return handle;
        }

        IntPtr ILink.TakeOpener() =>
            LinkOptionNative.TakeOpener("echocat", CreateHandle(), NativeEchocat.autd3_link_echocat_open);

        IntPtr ILegacyLink.TakeLegacyOpener() =>
            LinkOptionNative.TakeOpener("echocat", CreateHandle(), NativeEchocat.autd3_link_echocat_open_legacy);
    }

    internal static class NativeEchocat
    {
        private const string Lib = "autd3_link_echocat";

        static NativeEchocat() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_echocat_option_new();

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_iface(IntPtr option, [MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_sync0_period(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_frame_phase(IntPtr option, [MarshalAs(UnmanagedType.I1)] bool hasFramePhase, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_pdu_timeout(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_state_transition_timeout(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_dc_static_sync_iterations(IntPtr option, uint value);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_dc_start_delay(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_sync_tolerance(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_sync_timeout(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_process_data_watchdog(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_echocat_option_set_spin_margin(IntPtr option, [MarshalAs(UnmanagedType.I1)] bool hasSpinMargin, ulong ns);

        [DllImport(Lib)]
        internal static extern void autd3_link_echocat_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_echocat_open(IntPtr option, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_echocat_open_legacy(IntPtr option, byte[] outErr, UIntPtr outErrLen);
    }
}
