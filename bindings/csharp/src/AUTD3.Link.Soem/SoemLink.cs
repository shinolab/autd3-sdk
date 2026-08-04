using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct SoemLinkOption : ILink, ILegacyLink
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

        public static SoemLinkOption SafeDefault(Interface? iface = null) =>
            FromPreset(iface, NativeSoem.autd3_link_soem_option_safe_default);

        public static SoemLinkOption PerformanceDefault(Interface? iface = null) =>
            FromPreset(iface, NativeSoem.autd3_link_soem_option_performance_default);

        private delegate IntPtr PresetFn();

        private static SoemLinkOption FromPreset(Interface? iface, PresetFn preset)
        {
            var handle = preset();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to get soem link option preset");
            }
            try
            {
                return new SoemLinkOption(
                    iface,
                    LinkOptionNative.GetDuration(handle, NativeSoem.autd3_link_soem_option_get_sync0_period),
                    LinkOptionNative.GetDuration(handle, NativeSoem.autd3_link_soem_option_get_sync0_shift),
                    LinkOptionNative.GetDuration(handle, NativeSoem.autd3_link_soem_option_get_sync_tolerance),
                    LinkOptionNative.GetDuration(handle, NativeSoem.autd3_link_soem_option_get_sync_timeout));
            }
            finally
            {
                NativeSoem.autd3_link_soem_option_free(handle);
            }
        }

        private IntPtr CreateHandle()
        {
            var handle = NativeSoem.autd3_link_soem_option_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create soem link option");
            }
            try
            {
                LinkOptionNative.Apply("iface", NativeSoem.autd3_link_soem_option_set_iface(handle, Iface.NameValue));
                LinkOptionNative.SetDuration(handle, "sync0Period", Sync0Period, NativeSoem.autd3_link_soem_option_set_sync0_period);
                LinkOptionNative.SetDuration(handle, "sync0Shift", Sync0Shift, NativeSoem.autd3_link_soem_option_set_sync0_shift);
                LinkOptionNative.SetDuration(handle, "syncTolerance", SyncTolerance, NativeSoem.autd3_link_soem_option_set_sync_tolerance);
                LinkOptionNative.SetDuration(handle, "syncTimeout", SyncTimeout, NativeSoem.autd3_link_soem_option_set_sync_timeout);
            }
            catch
            {
                NativeSoem.autd3_link_soem_option_free(handle);
                throw;
            }
            return handle;
        }

        IntPtr ILink.TakeOpener() =>
            LinkOptionNative.TakeOpener("soem", CreateHandle(), NativeSoem.autd3_link_soem_open);

        IntPtr ILegacyLink.TakeLegacyOpener() =>
            LinkOptionNative.TakeOpener("soem", CreateHandle(), NativeSoem.autd3_link_soem_open_legacy);
    }

    internal static class NativeSoem
    {
        private const string Lib = "autd3_link_soem";

        static NativeSoem() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_soem_option_new();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_soem_option_safe_default();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_soem_option_performance_default();

        [DllImport(Lib)]
        internal static extern int autd3_link_soem_option_set_iface(IntPtr option, [MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName);

        [DllImport(Lib)]
        internal static extern int autd3_link_soem_option_set_sync0_period(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_soem_option_get_sync0_period(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_soem_option_set_sync0_shift(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_soem_option_get_sync0_shift(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_soem_option_set_sync_tolerance(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_soem_option_get_sync_tolerance(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_soem_option_set_sync_timeout(IntPtr option, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_soem_option_get_sync_timeout(IntPtr option, out ulong ns);

        [DllImport(Lib)]
        internal static extern void autd3_link_soem_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_soem_open(IntPtr option, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_soem_open_legacy(IntPtr option, byte[] outErr, UIntPtr outErrLen);
    }
}
