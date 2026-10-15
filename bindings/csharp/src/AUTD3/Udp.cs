using System;
using System.Runtime.InteropServices;
using System.Threading;

namespace AUTD3
{
    internal static class UdpOptionDefaults
    {
        internal static readonly TimeSpan? Heartbeat;
        internal static readonly TimeSpan ReplyTimeout;
        internal static readonly TimeSpan LostTimeout;
        internal static readonly TimeSpan ResponseTimeout;
        internal static readonly TimeSpan EnumerationTimeout;
        internal static readonly TimeSpan SyncTimeout;
        internal static readonly ulong? SendBuffer;
        internal static readonly TimeSpan? TimerResolution;

        static UdpOptionDefaults()
        {
            var handle = UdpOption.NewHandle();
            try
            {
                Heartbeat = UdpOption.ReadHeartbeat(handle);
                ReplyTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_reply_timeout);
                LostTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_lost_timeout);
                ResponseTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_response_timeout);
                EnumerationTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_enumeration_timeout);
                SyncTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_sync_timeout);
                SendBuffer = UdpOption.ReadSendBuffer(handle);
                TimerResolution = UdpOption.ReadTimerResolution(handle);
            }
            finally
            {
                NativeClient.autd3_udp_option_free(handle);
            }
        }
    }

    public readonly struct UdpOption
    {
        private readonly bool _heartbeatSet;
        private readonly TimeSpan? _heartbeat;

        public Interface Iface { get; init; }

        public TimeSpan? Heartbeat
        {
            get => _heartbeatSet ? _heartbeat : UdpOptionDefaults.Heartbeat;
            init
            {
                _heartbeat = value;
                _heartbeatSet = true;
            }
        }

        private readonly TimeSpan? _replyTimeout;
        public TimeSpan ReplyTimeout { get => _replyTimeout ?? UdpOptionDefaults.ReplyTimeout; init => _replyTimeout = value; }
        private readonly TimeSpan? _lostTimeout;
        public TimeSpan LostTimeout { get => _lostTimeout ?? UdpOptionDefaults.LostTimeout; init => _lostTimeout = value; }
        public TimeSpan? RecoveryInterval { get; init; }
        private readonly TimeSpan? _responseTimeout;
        public TimeSpan ResponseTimeout { get => _responseTimeout ?? UdpOptionDefaults.ResponseTimeout; init => _responseTimeout = value; }
        private readonly TimeSpan? _enumerationTimeout;
        public TimeSpan EnumerationTimeout { get => _enumerationTimeout ?? UdpOptionDefaults.EnumerationTimeout; init => _enumerationTimeout = value; }
        private readonly TimeSpan? _syncTimeout;
        public TimeSpan SyncTimeout { get => _syncTimeout ?? UdpOptionDefaults.SyncTimeout; init => _syncTimeout = value; }
        public float? SendRateLimit { get; init; }

        private readonly bool _sendBufferSet;
        private readonly ulong? _sendBuffer;

        public ulong? SendBuffer
        {
            get => _sendBufferSet ? _sendBuffer : UdpOptionDefaults.SendBuffer;
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
            get => _timerResolutionSet ? _timerResolution : UdpOptionDefaults.TimerResolution;
            init
            {
                _timerResolution = value;
                _timerResolutionSet = true;
            }
        }

        internal static IntPtr NewHandle()
        {
            var handle = NativeClient.autd3_udp_option_new();
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
                    { IsSimulator: true } => NativeClient.autd3_udp_option_set_iface_simulator(handle),
                    { AddrValue: { } addr } => NativeClient.autd3_udp_option_set_iface_addr(handle, addr),
                    _ => NativeClient.autd3_udp_option_set_iface(handle, Iface.NameValue),
                });
                OptionNative.Apply("heartbeat", Heartbeat switch
                {
                    null => NativeClient.autd3_udp_option_set_heartbeat(handle, 0),
                    { } interval when interval > TimeSpan.Zero => NativeClient.autd3_udp_option_set_heartbeat(handle, OptionNative.ToNanos(interval)),
                    _ => -1,
                });
                OptionNative.SetRequiredDuration(handle, "replyTimeout", ReplyTimeout, NativeClient.autd3_udp_option_set_reply_timeout);
                OptionNative.SetRequiredDuration(handle, "lostTimeout", LostTimeout, NativeClient.autd3_udp_option_set_lost_timeout);
                OptionNative.Apply("recoveryInterval", RecoveryInterval switch
                {
                    null => NativeClient.autd3_udp_option_set_recovery_interval(handle, 0),
                    { } interval when interval > TimeSpan.Zero => NativeClient.autd3_udp_option_set_recovery_interval(handle, OptionNative.ToNanos(interval)),
                    _ => -1,
                });
                OptionNative.SetRequiredDuration(handle, "responseTimeout", ResponseTimeout, NativeClient.autd3_udp_option_set_response_timeout);
                OptionNative.SetRequiredDuration(handle, "enumerationTimeout", EnumerationTimeout, NativeClient.autd3_udp_option_set_enumeration_timeout);
                OptionNative.SetRequiredDuration(handle, "syncTimeout", SyncTimeout, NativeClient.autd3_udp_option_set_sync_timeout);
                OptionNative.Apply("sendRateLimit", SendRateLimit switch
                {
                    null => NativeClient.autd3_udp_option_set_send_rate_limit(handle, 0),
                    { } percent when percent > 0 => NativeClient.autd3_udp_option_set_send_rate_limit(handle, percent),
                    _ => -1,
                });
                OptionNative.Apply("sendBuffer", SendBuffer switch
                {
                    null => NativeClient.autd3_udp_option_set_send_buffer(handle, 0),
                    { } bytes when bytes > 0 => NativeClient.autd3_udp_option_set_send_buffer(handle, bytes),
                    _ => -1,
                });
                OptionNative.Apply("timerResolution", TimerResolution switch
                {
                    null => NativeClient.autd3_udp_option_set_timer_resolution(handle, 0),
                    { } resolution when resolution > TimeSpan.Zero => NativeClient.autd3_udp_option_set_timer_resolution(handle, OptionNative.ToNanos(resolution)),
                    _ => -1,
                });
            }
            catch
            {
                NativeClient.autd3_udp_option_free(handle);
                throw;
            }
            return handle;
        }

        internal static UdpOption FromHandle(IntPtr handle) => new UdpOption
        {
            Iface = ReadAddr(handle) is { } addr ? Interface.Addr(addr) : Interface.Auto,
            Heartbeat = ReadHeartbeat(handle),
            ReplyTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_reply_timeout),
            LostTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_lost_timeout),
            RecoveryInterval = ReadRecoveryInterval(handle),
            ResponseTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_response_timeout),
            EnumerationTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_enumeration_timeout),
            SyncTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_sync_timeout),
            SendRateLimit = ReadSendRateLimit(handle),
            SendBuffer = ReadSendBuffer(handle),
            TimerResolution = ReadTimerResolution(handle),
        };

        internal static UdpOption Defaults()
        {
            var handle = NewHandle();
            try
            {
                return FromHandle(handle);
            }
            finally
            {
                NativeClient.autd3_udp_option_free(handle);
            }
        }

        internal static TimeSpan? ReadHeartbeat(IntPtr handle)
        {
            var heartbeat = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_heartbeat);
            return heartbeat == TimeSpan.Zero ? null : heartbeat;
        }

        private static TimeSpan? ReadRecoveryInterval(IntPtr handle)
        {
            var interval = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_recovery_interval);
            return interval == TimeSpan.Zero ? null : interval;
        }

        internal static TimeSpan? ReadTimerResolution(IntPtr handle)
        {
            var resolution = OptionNative.GetDuration(handle, NativeClient.autd3_udp_option_get_timer_resolution);
            return resolution == TimeSpan.Zero ? null : resolution;
        }

        internal static ulong? ReadSendBuffer(IntPtr handle)
        {
            OptionNative.Apply("sendBuffer", NativeClient.autd3_udp_option_get_send_buffer(handle, out var bytes));
            return bytes == 0 ? null : bytes;
        }

        private static float? ReadSendRateLimit(IntPtr handle)
        {
            OptionNative.Apply("sendRateLimit", NativeClient.autd3_udp_option_get_send_rate_limit(handle, out var percent));
            return percent == 0 ? null : percent;
        }

        private static string? ReadAddr(IntPtr handle)
        {
            OptionNative.Apply("iface", NativeClient.autd3_udp_option_get_iface_addr(handle, out var addr));
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

        public UdpOption Option()
        {
            var handle = NativeClient.autd3_udp_emulator_option(Handle);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to read the udp emulator option");
            }
            try
            {
                return UdpOption.FromHandle(handle);
            }
            finally
            {
                NativeClient.autd3_udp_option_free(handle);
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
