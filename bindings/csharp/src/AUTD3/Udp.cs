using System;
using System.Runtime.InteropServices;
using System.Threading;

namespace AUTD3
{
    internal static class TransportOptionDefaults
    {
        internal static readonly TimeSpan? Heartbeat;
        internal static readonly TimeSpan ReplyTimeout;
        internal static readonly TimeSpan LostTimeout;
        internal static readonly TimeSpan ResponseTimeout;
        internal static readonly TimeSpan EnumerationTimeout;
        internal static readonly TimeSpan SyncTimeout;
        internal static readonly uint? SendBuffer;
        internal static readonly TimeSpan? TimerResolution;

        static TransportOptionDefaults()
        {
            var handle = TransportOption.NewHandle();
            try
            {
                Heartbeat = TransportOption.ReadHeartbeat(handle);
                ReplyTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_reply_timeout);
                LostTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_lost_timeout);
                ResponseTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_response_timeout);
                EnumerationTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_enumeration_timeout);
                SyncTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_sync_timeout);
                SendBuffer = TransportOption.ReadSendBuffer(handle);
                TimerResolution = TransportOption.ReadTimerResolution(handle);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }
        }
    }

    public readonly struct TransportOption
    {
        private readonly bool _heartbeatSet;
        private readonly TimeSpan? _heartbeat;

        public Interface Iface { get; init; }

        public TimeSpan? Heartbeat
        {
            get => _heartbeatSet ? _heartbeat : TransportOptionDefaults.Heartbeat;
            init
            {
                _heartbeat = value;
                _heartbeatSet = true;
            }
        }

        private readonly TimeSpan? _replyTimeout;
        public TimeSpan ReplyTimeout { get => _replyTimeout ?? TransportOptionDefaults.ReplyTimeout; init => _replyTimeout = value; }
        private readonly TimeSpan? _lostTimeout;
        public TimeSpan LostTimeout { get => _lostTimeout ?? TransportOptionDefaults.LostTimeout; init => _lostTimeout = value; }
        private readonly TimeSpan? _responseTimeout;
        public TimeSpan ResponseTimeout { get => _responseTimeout ?? TransportOptionDefaults.ResponseTimeout; init => _responseTimeout = value; }
        private readonly TimeSpan? _enumerationTimeout;
        public TimeSpan EnumerationTimeout { get => _enumerationTimeout ?? TransportOptionDefaults.EnumerationTimeout; init => _enumerationTimeout = value; }
        private readonly TimeSpan? _syncTimeout;
        public TimeSpan SyncTimeout { get => _syncTimeout ?? TransportOptionDefaults.SyncTimeout; init => _syncTimeout = value; }
        public float? SendRateLimit { get; init; }

        private readonly bool _sendBufferSet;
        private readonly uint? _sendBuffer;

        public uint? SendBuffer
        {
            get => _sendBufferSet ? _sendBuffer : TransportOptionDefaults.SendBuffer;
            init
            {
                _sendBuffer = value;
                _sendBufferSet = true;
            }
        }

        private readonly bool _timerResolutionSet;
        private readonly TimeSpan? _timerResolution;

        public TimeSpan? TimerResolution
        {
            get => _timerResolutionSet ? _timerResolution : TransportOptionDefaults.TimerResolution;
            init
            {
                _timerResolution = value;
                _timerResolutionSet = true;
            }
        }

        internal static IntPtr NewHandle()
        {
            var handle = NativeClient.autd3_transport_option_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create udp option");
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
                    { IsSimulator: true } => NativeClient.autd3_transport_option_set_iface_simulator(handle),
                    { AddrValue: { } addr } => NativeClient.autd3_transport_option_set_iface_addr(handle, addr),
                    _ => NativeClient.autd3_transport_option_set_iface(handle, Iface.NameValue),
                });
                OptionNative.Apply("heartbeat", Heartbeat switch
                {
                    null => NativeClient.autd3_transport_option_set_heartbeat(handle, 0),
                    { } interval when interval > TimeSpan.Zero => NativeClient.autd3_transport_option_set_heartbeat(handle, OptionNative.ToNanos(interval)),
                    _ => -1,
                });
                OptionNative.SetRequiredDuration(handle, "replyTimeout", ReplyTimeout, NativeClient.autd3_transport_option_set_reply_timeout);
                OptionNative.SetRequiredDuration(handle, "lostTimeout", LostTimeout, NativeClient.autd3_transport_option_set_lost_timeout);
                OptionNative.SetRequiredDuration(handle, "responseTimeout", ResponseTimeout, NativeClient.autd3_transport_option_set_response_timeout);
                OptionNative.SetRequiredDuration(handle, "enumerationTimeout", EnumerationTimeout, NativeClient.autd3_transport_option_set_enumeration_timeout);
                OptionNative.SetRequiredDuration(handle, "syncTimeout", SyncTimeout, NativeClient.autd3_transport_option_set_sync_timeout);
                OptionNative.Apply("sendRateLimit", SendRateLimit switch
                {
                    null => NativeClient.autd3_transport_option_set_send_rate_limit(handle, 0),
                    { } percent when percent > 0 => NativeClient.autd3_transport_option_set_send_rate_limit(handle, percent),
                    _ => -1,
                });
                OptionNative.Apply("sendBuffer", SendBuffer switch
                {
                    null => NativeClient.autd3_transport_option_set_send_buffer(handle, 0),
                    { } bytes when bytes > 0 => NativeClient.autd3_transport_option_set_send_buffer(handle, bytes),
                    _ => -1,
                });
                OptionNative.Apply("timerResolution", TimerResolution switch
                {
                    null => NativeClient.autd3_transport_option_set_timer_resolution(handle, 0),
                    { } resolution when resolution > TimeSpan.Zero => NativeClient.autd3_transport_option_set_timer_resolution(handle, OptionNative.ToNanos(resolution)),
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
            Heartbeat = ReadHeartbeat(handle),
            ReplyTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_reply_timeout),
            LostTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_lost_timeout),
            ResponseTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_response_timeout),
            EnumerationTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_enumeration_timeout),
            SyncTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_sync_timeout),
            SendRateLimit = ReadSendRateLimit(handle),
            SendBuffer = ReadSendBuffer(handle),
            TimerResolution = ReadTimerResolution(handle),
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

        internal static TimeSpan? ReadHeartbeat(IntPtr handle)
        {
            var heartbeat = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_heartbeat);
            return heartbeat == TimeSpan.Zero ? null : heartbeat;
        }

        internal static TimeSpan? ReadTimerResolution(IntPtr handle)
        {
            var resolution = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_timer_resolution);
            return resolution == TimeSpan.Zero ? null : resolution;
        }

        internal static uint? ReadSendBuffer(IntPtr handle)
        {
            OptionNative.Apply("sendBuffer", NativeClient.autd3_transport_option_get_send_buffer(handle, out var bytes));
            return bytes == 0 ? null : checked((uint)bytes);
        }

        private static float? ReadSendRateLimit(IntPtr handle)
        {
            OptionNative.Apply("sendRateLimit", NativeClient.autd3_transport_option_get_send_rate_limit(handle, out var percent));
            return percent == 0 ? null : percent;
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
