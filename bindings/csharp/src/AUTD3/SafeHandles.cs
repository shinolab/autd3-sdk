using System;

namespace AUTD3
{
    internal sealed class DatagramBuilderHandle : Autd3SafeHandle
    {
        internal DatagramBuilderHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeClient.autd3_datagram_builder_free(handle);
            return true;
        }
    }

    internal sealed class FramesHandle : Autd3SafeHandle
    {
        internal FramesHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeClient.autd3_datagrams_free(handle);
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

    internal sealed class CheckerHandle : Autd3SafeHandle
    {
        internal CheckerHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeClient.autd3_checker_free(handle);
            return true;
        }
    }

    internal sealed class LegacyDatagramBuilderHandle : Autd3SafeHandle
    {
        internal LegacyDatagramBuilderHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeLegacyClient.autd3_legacy_datagram_builder_free(handle);
            return true;
        }
    }

    internal sealed class LegacyFramesHandle : Autd3SafeHandle
    {
        internal LegacyFramesHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeLegacyClient.autd3_legacy_frames_free(handle);
            return true;
        }
    }

    internal sealed class LegacyClientHandle : Autd3SafeHandle
    {
        internal LegacyClientHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeLegacyClient.autd3_legacy_client_free(handle);
            return true;
        }
    }
}
