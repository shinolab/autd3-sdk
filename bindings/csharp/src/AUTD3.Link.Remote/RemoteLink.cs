using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct RemoteLinkOption : ILink, ILegacyLink
    {
        public string Addr { get; }
        public TimeSpan? Timeout { get; }

        public RemoteLinkOption(string addr, TimeSpan? timeout = null)
        {
            Addr = addr;
            Timeout = timeout;
        }

        public static RemoteLinkOption Discover(TimeSpan? timeout = null, string? instance = null)
        {
            var timeoutNs = timeout.HasValue ? LinkOptionNative.ToNanos(timeout.Value) : 0UL;
            var linkTimeoutNs = 0UL;
            var err = IntPtr.Zero;
            var found = NativeRemote.autd3_link_remote_discover(timeoutNs, instance, ref linkTimeoutNs, ref err);
            if (found != IntPtr.Zero)
            {
                try
                {
                    var addr = Marshal.PtrToStringUTF8(found) ?? throw new Autd3Exception("mDNS discovery returned nothing");
                    return new RemoteLinkOption(addr, linkTimeoutNs == 0 ? null : LinkOptionNative.FromNanos(linkTimeoutNs));
                }
                finally
                {
                    NativeRemote.autd3_link_remote_free_string(found);
                }
            }
            var message = err == IntPtr.Zero ? "mDNS discovery failed" : Marshal.PtrToStringUTF8(err) ?? "mDNS discovery failed";
            if (err != IntPtr.Zero)
            {
                NativeRemote.autd3_link_remote_free_string(err);
            }
            throw new Autd3Exception(message);
        }

        private IntPtr CreateHandle()
        {
            var handle = NativeRemote.autd3_link_remote_option_new(Addr);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create remote link option (invalid address?)");
            }
            try
            {
                LinkOptionNative.SetOptionalDuration(handle, "timeout", Timeout, NativeRemote.autd3_link_remote_option_set_timeout);
            }
            catch
            {
                NativeRemote.autd3_link_remote_option_free(handle);
                throw;
            }
            return handle;
        }

        IntPtr ILink.TakeOpener() =>
            LinkOptionNative.TakeOpener("remote", CreateHandle(), NativeRemote.autd3_link_remote_open);

        IntPtr ILegacyLink.TakeLegacyOpener() =>
            LinkOptionNative.TakeOpener("remote", CreateHandle(), NativeRemote.autd3_link_remote_open_legacy);
    }

    internal static class NativeRemote
    {
        private const string Lib = "autd3_link_remote";

        static NativeRemote() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_remote_option_new([MarshalAs(UnmanagedType.LPUTF8Str)] string addr);

        [DllImport(Lib)]
        internal static extern int autd3_link_remote_option_set_timeout(IntPtr option, [MarshalAs(UnmanagedType.I1)] bool hasValue, ulong ns);

        [DllImport(Lib)]
        internal static extern void autd3_link_remote_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_remote_open(IntPtr option, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_remote_open_legacy(IntPtr option, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_remote_discover(ulong timeoutNs, [MarshalAs(UnmanagedType.LPUTF8Str)] string? instance, ref ulong linkTimeoutNs, ref IntPtr err);

        [DllImport(Lib)]
        internal static extern void autd3_link_remote_free_string(IntPtr ptr);
    }
}
