using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public sealed class SoemLink : ILink
    {
        private IntPtr _opener;

        private SoemLink(IntPtr opener)
        {
            _opener = opener;
        }



        public static SoemLink Create(string? interfaceName = null)
        {
            var opener = NativeSoem.autd3_link_soem(interfaceName);
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create soem link");
            }
            return new SoemLink(opener);
        }

        public IntPtr TakeOpener()
        {
            var opener = _opener;
            _opener = IntPtr.Zero;
            return opener;
        }
    }

    internal static class NativeSoem
    {
        private const string Lib = "autd3_link_soem";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_soem([MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName);
    }
}
