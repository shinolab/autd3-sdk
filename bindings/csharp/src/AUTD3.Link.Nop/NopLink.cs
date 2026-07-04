using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public sealed class Nop : ILink
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
    }

    internal static class NativeNop
    {
        private const string Lib = "autd3_link_nop";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_nop();
    }
}
