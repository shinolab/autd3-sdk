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
    }
}
