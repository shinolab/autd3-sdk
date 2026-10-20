using System;
using System.Runtime.InteropServices;
using System.Threading;

namespace AUTD3
{
    internal static class TransportOptionDefaults
    {
        internal static readonly TimeSpan RecoveryReplyTimeout;
        internal static readonly TimeSpan LostTimeout;
        internal static readonly TimeSpan ControlRequestTimeout;
        internal static readonly TimeSpan EnumerationTimeout;
        internal static readonly TimeSpan SyncTimeout;
        internal static readonly TimeSpan AckTimeout;
        internal static readonly float? SendRateLimitPercent;
        internal static readonly ulong? SendBufferBytes;

        static TransportOptionDefaults()
        {
            var handle = TransportOption.NewHandle();
            try
            {
                RecoveryReplyTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_recovery_reply_timeout);
                LostTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_lost_timeout);
                ControlRequestTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_control_request_timeout);
                EnumerationTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_enumeration_timeout);
                SyncTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_sync_timeout);
                AckTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_ack_timeout);
                SendRateLimitPercent = TransportOption.ReadSendRateLimitPercent(handle);
                SendBufferBytes = TransportOption.ReadSendBufferBytes(handle);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }
        }
    }

    public readonly struct TransportOption
    {
        public static readonly ushort SimulatorPort = NativeClient.autd3_params_simulator_port();

        public Interface Iface { get; init; }

        private readonly TimeSpan? _recoveryReplyTimeout;
        public TimeSpan RecoveryReplyTimeout { get => _recoveryReplyTimeout ?? TransportOptionDefaults.RecoveryReplyTimeout; init => _recoveryReplyTimeout = value; }
        private readonly TimeSpan? _lostTimeout;
        public TimeSpan LostTimeout { get => _lostTimeout ?? TransportOptionDefaults.LostTimeout; init => _lostTimeout = value; }

        private readonly TimeSpan? _controlRequestTimeout;
        public TimeSpan ControlRequestTimeout { get => _controlRequestTimeout ?? TransportOptionDefaults.ControlRequestTimeout; init => _controlRequestTimeout = value; }
        private readonly TimeSpan? _enumerationTimeout;
        public TimeSpan EnumerationTimeout { get => _enumerationTimeout ?? TransportOptionDefaults.EnumerationTimeout; init => _enumerationTimeout = value; }
        private readonly TimeSpan? _syncTimeout;
        public TimeSpan SyncTimeout { get => _syncTimeout ?? TransportOptionDefaults.SyncTimeout; init => _syncTimeout = value; }
        private readonly TimeSpan? _ackTimeout;
        public TimeSpan AckTimeout { get => _ackTimeout ?? TransportOptionDefaults.AckTimeout; init => _ackTimeout = value; }

        private readonly bool _sendRateLimitPercentSet;
        private readonly float? _sendRateLimitPercent;

        public float? SendRateLimitPercent
        {
            get => _sendRateLimitPercentSet ? _sendRateLimitPercent : TransportOptionDefaults.SendRateLimitPercent;
            init
            {
                _sendRateLimitPercent = value;
                _sendRateLimitPercentSet = true;
            }
        }

        private readonly bool _sendBufferBytesSet;
        private readonly ulong? _sendBufferBytes;

        public ulong? SendBufferBytes
        {
            get => _sendBufferBytesSet ? _sendBufferBytes : TransportOptionDefaults.SendBufferBytes;
            init
            {
                _sendBufferBytes = value;
                _sendBufferBytesSet = true;
            }
        }

        internal static IntPtr NewHandle()
        {
            var handle = NativeClient.autd3_transport_option_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create transport option");
            }
            return handle;
        }

        internal IntPtr CreateHandle()
        {
            var handle = NewHandle();
            try
            {
                OptionNative.Apply("iface", Iface switch
                {
                    { SimulatorPort: { } port } => NativeClient.autd3_transport_option_set_iface_simulator(handle, port),
                    { AddrValue: { } addr } => NativeClient.autd3_transport_option_set_iface_addr(handle, addr),
                    _ => NativeClient.autd3_transport_option_set_iface(handle, Iface.NameValue),
                });
                OptionNative.SetRequiredDuration(handle, "recoveryReplyTimeout", RecoveryReplyTimeout, NativeClient.autd3_transport_option_set_recovery_reply_timeout);
                OptionNative.SetRequiredDuration(handle, "lostTimeout", LostTimeout, NativeClient.autd3_transport_option_set_lost_timeout);
                OptionNative.SetRequiredDuration(handle, "controlRequestTimeout", ControlRequestTimeout, NativeClient.autd3_transport_option_set_control_request_timeout);
                OptionNative.SetRequiredDuration(handle, "enumerationTimeout", EnumerationTimeout, NativeClient.autd3_transport_option_set_enumeration_timeout);
                OptionNative.SetRequiredDuration(handle, "syncTimeout", SyncTimeout, NativeClient.autd3_transport_option_set_sync_timeout);
                OptionNative.SetRequiredDuration(handle, "ackTimeout", AckTimeout, NativeClient.autd3_transport_option_set_ack_timeout);
                OptionNative.Apply("sendRateLimitPercent", SendRateLimitPercent switch
                {
                    null => NativeClient.autd3_transport_option_set_send_rate_limit_percent(handle, false, 0),
                    { } percent => NativeClient.autd3_transport_option_set_send_rate_limit_percent(handle, true, percent),
                });
                OptionNative.Apply("sendBufferBytes", SendBufferBytes switch
                {
                    null => NativeClient.autd3_transport_option_set_send_buffer_bytes(handle, 0),
                    { } bytes when bytes > 0 => NativeClient.autd3_transport_option_set_send_buffer_bytes(handle, bytes),
                    _ => -1,
                });
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
            Iface = ReadAddr(handle) is { } addr ? Interface.Addr(addr) : Interface.Auto,
            RecoveryReplyTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_recovery_reply_timeout),
            LostTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_lost_timeout),
            ControlRequestTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_control_request_timeout),
            EnumerationTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_enumeration_timeout),
            SyncTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_sync_timeout),
            AckTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_ack_timeout),
            SendRateLimitPercent = ReadSendRateLimitPercent(handle),
            SendBufferBytes = ReadSendBufferBytes(handle),
        };

        internal static TransportOption Defaults()
        {
            var handle = NewHandle();
            try
            {
                return FromHandle(handle);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }
        }

        internal static ulong? ReadSendBufferBytes(IntPtr handle)
        {
            OptionNative.Apply("sendBufferBytes", NativeClient.autd3_transport_option_get_send_buffer_bytes(handle, out var bytes));
            return bytes == 0 ? null : bytes;
        }

        internal static float? ReadSendRateLimitPercent(IntPtr handle)
        {
            OptionNative.Apply("sendRateLimitPercent", NativeClient.autd3_transport_option_get_send_rate_limit_percent(handle, out var hasValue, out var percent));
            return hasValue ? percent : (float?)null;
        }

        private static string? ReadAddr(IntPtr handle)
        {
            OptionNative.Apply("iface", NativeClient.autd3_transport_option_get_iface_addr(handle, out var addr));
            if (addr == IntPtr.Zero)
            {
                return null;
            }
            try
            {
                return Marshal.PtrToStringUTF8(addr);
            }
            finally
            {
                NativeClient.autd3_udp_free_string(addr);
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
                    : $"failed to spawn the udp emulator: {reason}", Autd3ErrorCode.Network);
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

        public int NumDevices => (int)NativeClient.autd3_udp_emulator_num_devices(Handle);

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
