using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public sealed class RemoteLink : ILink
    {
        private IntPtr _opener;

        private RemoteLink(IntPtr opener)
        {
            _opener = opener;
        }

        public static RemoteLink Create(string addr)
        {
            var opener = NativeRemote.autd3_link_remote(addr);
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create remote link (invalid address?)");
            }
            return new RemoteLink(opener);
        }

        public IntPtr TakeOpener()
        {
            var opener = _opener;
            _opener = IntPtr.Zero;
            return opener;
        }
    }

    internal static class NativeRemote
    {
        private const string Lib = "autd3_link_remote";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_remote([MarshalAs(UnmanagedType.LPUTF8Str)] string addr);
    }
}
