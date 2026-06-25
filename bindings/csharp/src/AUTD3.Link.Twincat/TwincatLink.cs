using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public enum TwinCATRoute : byte
    {
        Auto = 0,
        Notify = 1,
        Ads = 2,
    }

    public sealed class TwinCATLink : ILink
    {
        private IntPtr _opener;

        private TwinCATLink(IntPtr opener)
        {
            _opener = opener;
        }

        public static TwinCATLink Local(TwinCATRoute route = TwinCATRoute.Auto)
        {
            var opener = NativeTwincat.autd3_link_twincat_local((byte)route);
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create twincat link");
            }
            return new TwinCATLink(opener);
        }

        public static TwinCATLink Remote(string addr, string amsNetId, TwinCATRoute route = TwinCATRoute.Auto)
        {
            var opener = NativeTwincat.autd3_link_twincat_remote(addr, amsNetId, (byte)route);
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
        internal static extern IntPtr autd3_link_twincat_local(byte route);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_twincat_remote([MarshalAs(UnmanagedType.LPUTF8Str)] string addr, [MarshalAs(UnmanagedType.LPUTF8Str)] string amsNetId, byte route);
    }
}
