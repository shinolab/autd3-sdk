using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public sealed class Nop : ILink, ILegacyLink
    {
        public Nop()
        {
        }

        IntPtr ILink.TakeOpener()
        {
            var opener = NativeNop.autd3_link_nop();
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create nop link");
            }
            return opener;
        }

        IntPtr ILegacyLink.TakeLegacyOpener()
        {
            var opener = NativeNop.autd3_link_nop_legacy();
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create nop link");
            }
            return opener;
        }
    }

    internal static class NativeNop
    {
        private const string Lib = "autd3_link_nop";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_nop();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_nop_legacy();
    }
}
