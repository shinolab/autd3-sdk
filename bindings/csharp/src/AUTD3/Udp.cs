using System;
using System.Runtime.InteropServices;
using System.Threading;

namespace AUTD3
{
    internal static class TransportOptionDefaults
    {
        internal static readonly TimeSpan OpenTimeout;
        internal static readonly TimeSpan RecoveryReplyTimeout;
        internal static readonly TimeSpan LostTimeout;
        internal static readonly TimeSpan AckTimeout;
        internal static readonly float? SendRateLimitPercent;
        internal static readonly ulong? SendBufferBytes;

        static TransportOptionDefaults()
        {
            var handle = TransportOption.NewHandle();
            try
            {
                OpenTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_open_timeout);
                RecoveryReplyTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_recovery_reply_timeout);
                LostTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_lost_timeout);
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
        public Interface Iface { get; init; }

        private readonly TimeSpan? _openTimeout;
        public TimeSpan OpenTimeout { get => _openTimeout ?? TransportOptionDefaults.OpenTimeout; init => _openTimeout = value; }
        private readonly TimeSpan? _recoveryReplyTimeout;
        public TimeSpan RecoveryReplyTimeout { get => _recoveryReplyTimeout ?? TransportOptionDefaults.RecoveryReplyTimeout; init => _recoveryReplyTimeout = value; }
        private readonly TimeSpan? _lostTimeout;
        public TimeSpan LostTimeout { get => _lostTimeout ?? TransportOptionDefaults.LostTimeout; init => _lostTimeout = value; }
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
                    { AddrValue: { } addr } => NativeClient.autd3_transport_option_set_iface_addr(handle, addr),
                    _ => NativeClient.autd3_transport_option_set_iface(handle, Iface.NameValue),
                });
                OptionNative.SetRequiredDuration(handle, "openTimeout", OpenTimeout, NativeClient.autd3_transport_option_set_open_timeout);
                OptionNative.SetRequiredDuration(handle, "recoveryReplyTimeout", RecoveryReplyTimeout, NativeClient.autd3_transport_option_set_recovery_reply_timeout);
                OptionNative.SetRequiredDuration(handle, "lostTimeout", LostTimeout, NativeClient.autd3_transport_option_set_lost_timeout);
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
            OpenTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_open_timeout),
            RecoveryReplyTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_recovery_reply_timeout),
            LostTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_transport_option_get_lost_timeout),
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
}
