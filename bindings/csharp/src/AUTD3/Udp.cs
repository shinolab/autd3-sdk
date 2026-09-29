using System;
using System.Runtime.InteropServices;

namespace AUTD3
{
    public readonly struct TransportOption
    {
        public Interface Iface { get; init; } = Interface.Auto;
        public string? Group { get; init; } = null;
        public TimeSpan? Heartbeat { get; init; } = null;
        public TimeSpan? ReplyTimeout { get; init; } = null;
        public TimeSpan? LostTimeout { get; init; } = null;
        public TimeSpan? ResponseTimeout { get; init; } = null;
        public TimeSpan? EnumerationTimeout { get; init; } = null;
        public TimeSpan? SyncTimeout { get; init; } = null;

        public TransportOption()
        {
        }

        internal IntPtr CreateHandle()
        {
            var handle = NativeClient.autd3_transport_option_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create udp option");
            }
            try
            {
                OptionNative.Apply("iface", NativeClient.autd3_transport_option_set_iface(handle, Iface.NameValue));
                OptionNative.Apply("group", NativeClient.autd3_transport_option_set_group(handle, Group));
                OptionNative.SetDuration(handle, "heartbeat", Heartbeat, NativeClient.autd3_transport_option_set_heartbeat);
                OptionNative.SetDuration(handle, "replyTimeout", ReplyTimeout, NativeClient.autd3_transport_option_set_reply_timeout);
                OptionNative.SetDuration(handle, "lostTimeout", LostTimeout, NativeClient.autd3_transport_option_set_lost_timeout);
                OptionNative.SetDuration(handle, "responseTimeout", ResponseTimeout, NativeClient.autd3_transport_option_set_response_timeout);
                OptionNative.SetDuration(handle, "enumerationTimeout", EnumerationTimeout, NativeClient.autd3_transport_option_set_enumeration_timeout);
                OptionNative.SetDuration(handle, "syncTimeout", SyncTimeout, NativeClient.autd3_transport_option_set_sync_timeout);
            }
            catch
            {
                NativeClient.autd3_transport_option_free(handle);
                throw;
            }
            return handle;
        }

        internal static TransportOption FromHandle(IntPtr handle) => new TransportOption
        {
            Group = ReadGroup(handle),
            Heartbeat = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_heartbeat),
            ReplyTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_reply_timeout),
            LostTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_lost_timeout),
            ResponseTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_response_timeout),
            EnumerationTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_enumeration_timeout),
            SyncTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_sync_timeout),
        };

        internal static TransportOption Defaults()
        {
            var handle = NativeClient.autd3_transport_option_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create udp option");
            }
            try
            {
                return FromHandle(handle);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }
        }

        private static string? ReadGroup(IntPtr handle)
        {
            OptionNative.Apply("group", NativeClient.autd3_transport_option_get_group(handle, out var group));
            if (group == IntPtr.Zero)
            {
                return null;
            }
            try
            {
                return Marshal.PtrToStringUTF8(group);
            }
            finally
            {
                NativeClient.autd3_udp_free_string(group);
            }
        }
    }

    public sealed class UdpEmulator : IDisposable
    {
        private readonly UdpEmulatorHandle _handle;

        public UdpEmulator(int numDevices)
        {
            if (numDevices < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(numDevices));
            }
            var err = new byte[NativeAbi.ErrorBufferLength];
            var raw = NativeClient.autd3_udp_emulator_spawn((UIntPtr)numDevices, err, (UIntPtr)err.Length);
            if (raw == IntPtr.Zero)
            {
                var reason = NativeUtil.Utf8(err);
                throw new Autd3Exception(reason.Length == 0
                    ? "failed to spawn the udp emulator"
                    : $"failed to spawn the udp emulator: {reason}");
            }
            _handle = new UdpEmulatorHandle(raw);
        }

        public TransportOption Option()
        {
            var handle = NativeClient.autd3_udp_emulator_option(Handle);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to read the udp emulator option");
            }
            try
            {
                return TransportOption.FromHandle(handle);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }
        }

        public void Reboot(int index)
        {
            if (index < 0 || NativeClient.autd3_udp_emulator_reboot(Handle, (UIntPtr)index) != 0)
            {
                throw new ArgumentOutOfRangeException(nameof(index));
            }
        }

        private UdpEmulatorHandle Handle => _handle.IsClosed ? throw new ObjectDisposedException(nameof(UdpEmulator)) : _handle;

        public void Dispose() => _handle.Dispose();
    }

    internal sealed class UdpEmulatorHandle : Autd3SafeHandle
    {
        internal UdpEmulatorHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeClient.autd3_udp_emulator_free(handle);
            return true;
        }
    }
}
