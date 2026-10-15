using System;

namespace AUTD3
{
    internal static class ServerLinkDefaults
    {
        internal static readonly ushort Port;
        internal static readonly TimeSpan Timeout;

        static ServerLinkDefaults()
        {
            var handle = ServerLink.NewHandle();
            try
            {
                OptionNative.Apply("port", NativeClient.autd3_server_link_get_port(handle, out Port));
                Timeout = OptionNative.GetDuration(handle, NativeClient.autd3_server_link_get_timeout_ns);
            }
            finally
            {
                NativeClient.autd3_server_link_free(handle);
            }
        }
    }

    public readonly struct ServerLink
    {
        public static readonly ushort ServerPort = NativeClient.autd3_params_server_port();

        private readonly ushort? _port;
        public ushort Port { get => _port ?? ServerLinkDefaults.Port; init => _port = value; }
        private readonly TimeSpan? _timeout;
        public TimeSpan Timeout { get => _timeout ?? ServerLinkDefaults.Timeout; init => _timeout = value; }

        internal static IntPtr NewHandle()
        {
            var handle = NativeClient.autd3_server_link_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create server link");
            }
            return handle;
        }

        internal IntPtr CreateHandle()
        {
            var handle = NewHandle();
            try
            {
                OptionNative.Apply("port", NativeClient.autd3_server_link_set_port(handle, Port));
                OptionNative.SetRequiredDuration(handle, "timeout", Timeout, NativeClient.autd3_server_link_set_timeout_ns);
                return handle;
            }
            catch
            {
                NativeClient.autd3_server_link_free(handle);
                throw;
            }
        }
    }
}
