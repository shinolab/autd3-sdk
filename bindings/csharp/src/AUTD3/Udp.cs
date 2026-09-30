using System;
using System.Runtime.InteropServices;
using System.Threading;

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

    public sealed class Driver : IDisposable
    {
        private readonly DriverHandle _handle;

        private Driver(IntPtr handle)
        {
            _handle = new DriverHandle(handle);
        }

        public static (Driver Driver, Connector Connector) Open(TransportOption option, int numDevices)
        {
            if (numDevices < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(numDevices));
            }
            var optionHandle = option.CreateHandle();
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeClient.autd3_driver_open(optionHandle, (UIntPtr)numDevices, out var driver, out var connector, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            return (new Driver(driver), new Connector(connector));
        }

        public void Run()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            if (NativeClient.autd3_driver_run(Handle, err, (UIntPtr)err.Length) != 0)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
        }

        public bool Poll(out TimeSpan wait)
        {
            switch (NativeClient.autd3_driver_poll(Handle, out var ns))
            {
                case 0:
                    wait = TimeSpan.FromTicks((long)Math.Min(ns / 100, (ulong)TimeSpan.MaxValue.Ticks));
                    return true;
                case 1:
                    wait = TimeSpan.Zero;
                    return false;
                default:
                    throw new Autd3Exception("the driver is already being driven by another thread");
            }
        }

        public void Wait(TimeSpan wait)
        {
            var ns = wait <= TimeSpan.Zero ? 0UL : OptionNative.ToNanos(wait);
            if (NativeClient.autd3_driver_wait(Handle, ns) != 0)
            {
                throw new Autd3Exception("the driver is already being driven by another thread");
            }
        }

        public Checker StateChecker()
        {
            var checker = NativeClient.autd3_driver_state_checker(Handle);
            if (checker == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create checker");
            }
            return new Checker(checker);
        }

        private DriverHandle Handle => _handle.IsClosed ? throw new ObjectDisposedException(nameof(Driver)) : _handle;

        public void Dispose() => _handle.Dispose();
    }

    public sealed class Connector : IDisposable
    {
        private IntPtr _handle;

        internal Connector(IntPtr handle)
        {
            _handle = handle;
        }

        internal IntPtr Take()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("the connector has already been used");
            }
            GC.SuppressFinalize(this);
            return handle;
        }

        public void Dispose()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_connector_free(handle);
            }
            GC.SuppressFinalize(this);
        }

        ~Connector()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_connector_free(handle);
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
