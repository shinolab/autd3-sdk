using System;

namespace AUTD3
{
    internal sealed class FramesHandle : Autd3SafeHandle
    {
        internal FramesHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeClient.autd3_frames_free(handle);
            return true;
        }
    }

    internal sealed class ClientHandle : Autd3SafeHandle
    {
        internal ClientHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeClient.autd3_client_free(handle);
            return true;
        }
    }

    internal sealed class TracingGuardHandle : Autd3SafeHandle
    {
        internal TracingGuardHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeClient.autd3_tracing_guard_free(handle);
            return true;
        }
    }
}
