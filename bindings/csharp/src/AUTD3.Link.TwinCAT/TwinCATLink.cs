using System;
using System.Runtime.InteropServices;

namespace AUTD3.Link
{
    public readonly struct Timeouts
    {
        public TimeSpan? Connect { get; }
        public TimeSpan? Read { get; }
        public TimeSpan? Write { get; }

        public Timeouts(TimeSpan? connect = null, TimeSpan? read = null, TimeSpan? write = null)
        {
            Connect = connect;
            Read = read;
            Write = write;
        }
    }

    public readonly struct TwinCATLinkOption : ILink
    {
        private readonly bool _isRemote;
        private readonly string? _addr;
        private readonly string? _amsNetId;

        public Timeouts Timeouts { get; }

        private TwinCATLinkOption(bool isRemote, string? addr, string? amsNetId, Timeouts timeouts)
        {
            _isRemote = isRemote;
            _addr = addr;
            _amsNetId = amsNetId;
            Timeouts = timeouts;
        }

        public static TwinCATLinkOption Local() => LocalWithTimeouts(default);

        public static TwinCATLinkOption LocalWithTimeouts(Timeouts timeouts) =>
            new TwinCATLinkOption(false, null, null, timeouts);

        public static TwinCATLinkOption Remote(string addr, string amsNetId) =>
            RemoteWithTimeouts(addr, amsNetId, default);

        public static TwinCATLinkOption RemoteWithTimeouts(string addr, string amsNetId, Timeouts timeouts) =>
            new TwinCATLinkOption(true, addr, amsNetId, timeouts);

        IntPtr ILink.TakeOpener()
        {
            var opener = _isRemote
                ? NativeTwincat.autd3_link_twincat_remote(
                    _addr!, _amsNetId!,
                    Timeouts.Connect.HasValue, (ulong)(Timeouts.Connect?.Ticks * 100 ?? 0),
                    Timeouts.Read.HasValue, (ulong)(Timeouts.Read?.Ticks * 100 ?? 0),
                    Timeouts.Write.HasValue, (ulong)(Timeouts.Write?.Ticks * 100 ?? 0))
                : NativeTwincat.autd3_link_twincat_local(
                    Timeouts.Connect.HasValue, (ulong)(Timeouts.Connect?.Ticks * 100 ?? 0),
                    Timeouts.Read.HasValue, (ulong)(Timeouts.Read?.Ticks * 100 ?? 0),
                    Timeouts.Write.HasValue, (ulong)(Timeouts.Write?.Ticks * 100 ?? 0));
            if (opener == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create twincat link (invalid address or AMS Net Id?)");
            }
            return opener;
        }
    }

    internal static class NativeTwincat
    {
        private const string Lib = "autd3_link_twincat";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_twincat_local(
            [MarshalAs(UnmanagedType.I1)] bool hasConnect, ulong connectNs,
            [MarshalAs(UnmanagedType.I1)] bool hasRead, ulong readNs,
            [MarshalAs(UnmanagedType.I1)] bool hasWrite, ulong writeNs);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_twincat_remote(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string addr,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string amsNetId,
            [MarshalAs(UnmanagedType.I1)] bool hasConnect, ulong connectNs,
            [MarshalAs(UnmanagedType.I1)] bool hasRead, ulong readNs,
            [MarshalAs(UnmanagedType.I1)] bool hasWrite, ulong writeNs);
    }
}
