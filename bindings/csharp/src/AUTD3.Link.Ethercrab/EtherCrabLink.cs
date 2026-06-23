using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public sealed class EtherCrabLink : ILink
    {
        private IntPtr _opener;

        private EtherCrabLink(IntPtr opener)
        {
            _opener = opener;
        }



        public static EtherCrabLink Create(string? interfaceName = null)
        {
            var opener = NativeEthercrab.autd3_link_ethercrab(interfaceName);
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create ethercrab link");
            }
            return new EtherCrabLink(opener);
        }

        public IntPtr TakeOpener()
        {
            var opener = _opener;
            _opener = IntPtr.Zero;
            return opener;
        }
    }

    internal static class NativeEthercrab
    {
        private const string Lib = "autd3_link_ethercrab";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_ethercrab([MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName);
    }
}
