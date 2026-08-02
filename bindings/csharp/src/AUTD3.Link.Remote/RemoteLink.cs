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

        public static string Discover(TimeSpan? timeout = null, string? instance = null)
        {
            var timeoutNs = (ulong)(timeout?.Ticks * 100 ?? 0);
            var err = IntPtr.Zero;
            var found = NativeRemote.autd3_link_remote_discover(timeoutNs, instance, ref err);
            if (found != IntPtr.Zero)
            {
                try
                {
                    return Marshal.PtrToStringUTF8(found) ?? throw new Autd3Exception("mDNS discovery returned nothing");
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

        IntPtr ILink.TakeOpener()
        {
            var timeoutNs = (ulong)(Timeout?.Ticks * 100 ?? 0);
            var opener = NativeRemote.autd3_link_remote(Addr, timeoutNs);
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create remote link (invalid address?)");
            }
            return opener;
        }

        IntPtr ILegacyLink.TakeLegacyOpener()
        {
            var timeoutNs = (ulong)(Timeout?.Ticks * 100 ?? 0);
            var opener = NativeRemote.autd3_link_remote_legacy(Addr, timeoutNs);
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create remote link (invalid address?)");
            }
            return opener;
        }
    }

    internal static class NativeRemote
    {
        private const string Lib = "autd3_link_remote";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_remote([MarshalAs(UnmanagedType.LPUTF8Str)] string addr, ulong timeoutNs);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_remote_legacy([MarshalAs(UnmanagedType.LPUTF8Str)] string addr, ulong timeoutNs);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_remote_discover(ulong timeoutNs, [MarshalAs(UnmanagedType.LPUTF8Str)] string? instance, ref IntPtr err);

        [DllImport(Lib)]
        internal static extern void autd3_link_remote_free_string(IntPtr ptr);
    }
}
