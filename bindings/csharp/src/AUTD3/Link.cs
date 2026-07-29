using System;

namespace AUTD3
{


    public interface ILink
    {

        IntPtr TakeOpener();
    }

    public interface ILegacyLink
    {

        IntPtr TakeLegacyOpener();
    }
}
