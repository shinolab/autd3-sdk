using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public sealed class TwinCATLink : ILink
    {
        private IntPtr _opener;

        private TwinCATLink(IntPtr opener)
        {
            _opener = opener;
        }

        public static TwinCATLink Local(TimeSpan? connectTimeout = null, TimeSpan? readTimeout = null, TimeSpan? writeTimeout = null)
        {
            var opener = NativeTwincat.autd3_link_twincat_local(
                connectTimeout.HasValue, (ulong)(connectTimeout?.Ticks * 100 ?? 0),
                readTimeout.HasValue, (ulong)(readTimeout?.Ticks * 100 ?? 0),
                writeTimeout.HasValue, (ulong)(writeTimeout?.Ticks * 100 ?? 0));
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create twincat link");
            }
            return new TwinCATLink(opener);
        }

        public static TwinCATLink Remote(string addr, string amsNetId, TimeSpan? connectTimeout = null, TimeSpan? readTimeout = null, TimeSpan? writeTimeout = null)
        {
            var opener = NativeTwincat.autd3_link_twincat_remote(
                addr, amsNetId,
                connectTimeout.HasValue, (ulong)(connectTimeout?.Ticks * 100 ?? 0),
                readTimeout.HasValue, (ulong)(readTimeout?.Ticks * 100 ?? 0),
                writeTimeout.HasValue, (ulong)(writeTimeout?.Ticks * 100 ?? 0));
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create twincat link (invalid address or AMS Net Id?)");
            }
            return new TwinCATLink(opener);
        }

        public IntPtr TakeOpener()
        {
            var opener = _opener;
            _opener = IntPtr.Zero;
            return opener;
        }
    }

    internal static class NativeTwincat
    {
        private const string Lib = "autd3_link_twincat";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_twincat_local(
            [MarshalAs(UnmanagedType.I1)] bool hasConnect, ulong connectNs,
            [MarshalAs(UnmanagedType.I1)] bool hasRead, ulong readNs,
            [MarshalAs(UnmanagedType.I1)] bool hasWrite, ulong writeNs);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_twincat_remote(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string addr,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string amsNetId,
            [MarshalAs(UnmanagedType.I1)] bool hasConnect, ulong connectNs,
            [MarshalAs(UnmanagedType.I1)] bool hasRead, ulong readNs,
            [MarshalAs(UnmanagedType.I1)] bool hasWrite, ulong writeNs);
    }
}
