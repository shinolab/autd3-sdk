using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public sealed class NopLink : ILink
    {
        private IntPtr _opener;

        private NopLink(IntPtr opener)
        {
            _opener = opener;
        }

        public static NopLink Create()
        {
            var opener = NativeNop.autd3_link_nop();
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create nop link");
            }
            return new NopLink(opener);
        }

        public IntPtr TakeOpener()
        {
            var opener = _opener;
            _opener = IntPtr.Zero;
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
