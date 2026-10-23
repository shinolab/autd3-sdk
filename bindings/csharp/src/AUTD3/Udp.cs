using System;
using System.Runtime.InteropServices;

namespace AUTD3
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct TransportOptionNative
    {
        public ulong OpenTimeoutNs;
        public ulong LostTimeoutNs;
        public ulong AckTimeoutNs;
        public ulong SendBufferBytes;
        public byte SyncOnOpen;
    }

    internal static class TransportOptionDefaults
    {
        internal static readonly TimeSpan OpenTimeout;
        internal static readonly TimeSpan LostTimeout;
        internal static readonly TimeSpan AckTimeout;
        internal static readonly ulong? SendBufferBytes;
        internal static readonly bool SyncOnOpen;

        static TransportOptionDefaults()
        {
            OptionNative.Apply("preset", NativeClient.autd3_transport_option_default(out var native));
            OpenTimeout = OptionNative.FromNanos(native.OpenTimeoutNs);
            LostTimeout = OptionNative.FromNanos(native.LostTimeoutNs);
            AckTimeout = OptionNative.FromNanos(native.AckTimeoutNs);
            SendBufferBytes = native.SendBufferBytes == 0 ? null : native.SendBufferBytes;
            SyncOnOpen = native.SyncOnOpen != 0;
        }
    }

    public readonly struct TransportOption
    {
        private const byte IfaceKindAuto = 0;
        private const byte IfaceKindName = 1;
        private const byte IfaceKindAddr = 2;

        public Interface Iface { get; init; }

        private readonly TimeSpan? _openTimeout;
        public TimeSpan OpenTimeout { get => _openTimeout ?? TransportOptionDefaults.OpenTimeout; init => _openTimeout = value; }
        private readonly TimeSpan? _lostTimeout;
        public TimeSpan LostTimeout { get => _lostTimeout ?? TransportOptionDefaults.LostTimeout; init => _lostTimeout = value; }
        private readonly TimeSpan? _ackTimeout;
        public TimeSpan AckTimeout { get => _ackTimeout ?? TransportOptionDefaults.AckTimeout; init => _ackTimeout = value; }

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

        private readonly bool? _syncOnOpen;
        public bool SyncOnOpen { get => _syncOnOpen ?? TransportOptionDefaults.SyncOnOpen; init => _syncOnOpen = value; }

        internal byte IfaceKind => Iface switch
        {
            { AddrValue: { } } => IfaceKindAddr,
            { NameValue: { } } => IfaceKindName,
            _ => IfaceKindAuto,
        };

        internal string? IfaceValue => Iface.AddrValue ?? Iface.NameValue;

        internal TransportOptionNative ToNative() => new TransportOptionNative
        {
            OpenTimeoutNs = OptionNative.RequiredNanos("openTimeout", OpenTimeout),
            LostTimeoutNs = OptionNative.RequiredNanos("lostTimeout", LostTimeout),
            AckTimeoutNs = OptionNative.RequiredNanos("ackTimeout", AckTimeout),
            SendBufferBytes = SendBufferBytes switch
            {
                null => 0,
                { } bytes when bytes > 0 => bytes,
                _ => throw OptionNative.OutOfRange("sendBufferBytes"),
            },
            SyncOnOpen = OptionNative.Flag(SyncOnOpen),
        };
    }
}
