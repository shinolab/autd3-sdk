using System;

namespace AUTD3
{


    public interface ICommand
    {
        internal IntPtr CreateOp();
    }
}
