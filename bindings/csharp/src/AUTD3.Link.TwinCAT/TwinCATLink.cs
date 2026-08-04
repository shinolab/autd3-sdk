using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct Timeouts
    {
        public TimeSpan? Connect { get; }
        public TimeSpan? Read { get; }
        public TimeSpan? Write { get; }

        public Timeouts(TimeSpan? connect = null, TimeSpan? read = null, TimeSpan? write = null)
        {
            Connect = connect;
            Read = read;
            Write = write;
        }
    }

    public readonly struct TwinCATLinkOption : ILink, ILegacyLink
    {
        private readonly bool _isRemote;
        private readonly string? _addr;
        private readonly string? _amsNetId;

        public Timeouts Timeouts { get; }

        private TwinCATLinkOption(bool isRemote, string? addr, string? amsNetId, Timeouts timeouts)
        {
            _isRemote = isRemote;
            _addr = addr;
            _amsNetId = amsNetId;
            Timeouts = timeouts;
        }

        public static TwinCATLinkOption Local() => LocalWithTimeouts(default);

        public static TwinCATLinkOption LocalWithTimeouts(Timeouts timeouts) =>
            new TwinCATLinkOption(false, null, null, timeouts);

        public static TwinCATLinkOption Remote(string addr, string amsNetId) =>
            RemoteWithTimeouts(addr, amsNetId, default);

        public static TwinCATLinkOption RemoteWithTimeouts(string addr, string amsNetId, Timeouts timeouts) =>
            new TwinCATLinkOption(true, addr, amsNetId, timeouts);

        private IntPtr CreateHandle()
        {
            var handle = _isRemote
                ? NativeTwincat.autd3_link_twincat_option_remote(_addr!, _amsNetId!)
                : NativeTwincat.autd3_link_twincat_option_local();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create twincat link option (invalid address or AMS Net Id?)");
            }
            try
            {
                LinkOptionNative.SetOptionalDuration(handle, "connect", Timeouts.Connect, NativeTwincat.autd3_link_twincat_option_set_connect_timeout);
                LinkOptionNative.SetOptionalDuration(handle, "read", Timeouts.Read, NativeTwincat.autd3_link_twincat_option_set_read_timeout);
                LinkOptionNative.SetOptionalDuration(handle, "write", Timeouts.Write, NativeTwincat.autd3_link_twincat_option_set_write_timeout);
            }
            catch
            {
                NativeTwincat.autd3_link_twincat_option_free(handle);
                throw;
            }
            return handle;
        }

        IntPtr ILink.TakeOpener() =>
            LinkOptionNative.TakeOpener("twincat", CreateHandle(), NativeTwincat.autd3_link_twincat_open);

        IntPtr ILegacyLink.TakeLegacyOpener() =>
            LinkOptionNative.TakeOpener("twincat", CreateHandle(), NativeTwincat.autd3_link_twincat_open_legacy);
    }

    internal static class NativeTwincat
    {
        private const string Lib = "autd3_link_twincat";

        static NativeTwincat() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_twincat_option_local();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_twincat_option_remote(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string addr,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string amsNetId);

        [DllImport(Lib)]
        internal static extern int autd3_link_twincat_option_set_connect_timeout(IntPtr option, [MarshalAs(UnmanagedType.I1)] bool hasValue, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_twincat_option_set_read_timeout(IntPtr option, [MarshalAs(UnmanagedType.I1)] bool hasValue, ulong ns);

        [DllImport(Lib)]
        internal static extern int autd3_link_twincat_option_set_write_timeout(IntPtr option, [MarshalAs(UnmanagedType.I1)] bool hasValue, ulong ns);

        [DllImport(Lib)]
        internal static extern void autd3_link_twincat_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_twincat_open(IntPtr option);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_twincat_open_legacy(IntPtr option);
    }
}
