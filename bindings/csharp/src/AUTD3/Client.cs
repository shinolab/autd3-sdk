using System;
using System.Threading;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace AUTD3
{
    public readonly struct FpgaState
    {
        public byte Raw { get; }

        public FpgaState(byte raw)
        {
            Raw = raw;
        }

        public bool IsThermalAsserted => NativeClient.autd3_fpga_state_is_thermal_asserted(Raw);
        public ModulationBank CurrentModBank => (ModulationBank)NativeClient.autd3_fpga_state_current_mod_bank(Raw);
        public PatternBank CurrentPatternBank => (PatternBank)NativeClient.autd3_fpga_state_current_pattern_bank(Raw);
        public bool IsPatternMode => NativeClient.autd3_fpga_state_is_pattern_mode(Raw);
        public bool IsStmMode => !IsPatternMode;
        public bool IsPatternStopped => NativeClient.autd3_fpga_state_is_pattern_stopped(Raw);
        public bool IsModStopped => NativeClient.autd3_fpga_state_is_mod_stopped(Raw);
        public bool IsTransitionPending => NativeClient.autd3_fpga_state_is_transition_pending(Raw);
        public bool IsFailsafeActive => NativeClient.autd3_fpga_state_is_failsafe_active(Raw);
    }

    public readonly struct Version : IEquatable<Version>
    {
        public byte Major { get; }
        public byte Minor { get; }
        public byte Patch { get; }

        public Version(byte major, byte minor, byte patch)
        {
            Major = major;
            Minor = minor;
            Patch = patch;
        }

        public bool IsUnknown => Major == 0 && Minor == 0 && Patch == 0;

        public override string ToString() => $"{Major}.{Minor}.{Patch}";

        public bool Equals(Version other) => Major == other.Major && Minor == other.Minor && Patch == other.Patch;

        public override bool Equals(object? obj) => obj is Version other && Equals(other);

        public override int GetHashCode() => HashCode.Combine(Major, Minor, Patch);

        public static bool operator ==(Version left, Version right) => left.Equals(right);

        public static bool operator !=(Version left, Version right) => !left.Equals(right);
    }

    public readonly struct FirmwareVersion : IEquatable<FirmwareVersion>
    {
        public Version Cpu { get; }
        public Version Fpga { get; }
        public bool IsEmulator { get; }
        public bool IsSupported { get; }

        internal FirmwareVersion(NativeClient.FirmwareVersionNative native)
        {
            Cpu = new Version(native.CpuMajor, native.CpuMinor, native.CpuPatch);
            Fpga = new Version(native.FpgaMajor, native.FpgaMinor, native.FpgaPatch);
            IsEmulator = native.IsEmulator != 0;
            IsSupported = native.IsSupported != 0;
        }

        public static (byte Major, byte Minor) SupportedSeries
        {
            get
            {
                if (NativeClient.autd3_firmware_version_supported_series(out var major, out var minor) != 0)
                {
                    throw new Autd3Exception("failed to read the supported firmware series");
                }
                return (major, minor);
            }
        }

        public override string ToString() =>
            $"CPU: {Cpu}, FPGA: {(Fpga.IsUnknown ? "unknown" : Fpga.ToString())}{(IsEmulator ? " [Emulator]" : string.Empty)}";

        public bool Equals(FirmwareVersion other) =>
            Cpu == other.Cpu && Fpga == other.Fpga && IsEmulator == other.IsEmulator && IsSupported == other.IsSupported;

        public override bool Equals(object? obj) => obj is FirmwareVersion other && Equals(other);

        public override int GetHashCode() => HashCode.Combine(Cpu, Fpga, IsEmulator, IsSupported);

        public static bool operator ==(FirmwareVersion left, FirmwareVersion right) => left.Equals(right);

        public static bool operator !=(FirmwareVersion left, FirmwareVersion right) => !left.Equals(right);
    }

    public sealed class BusStats : IDisposable
    {
        private readonly BusStatsHandle _handle;

        internal BusStats(IntPtr handle)
        {
            _handle = new BusStatsHandle(handle);
        }

        private BusStatsHandle Handle
        {
            get
            {
                if (_handle.IsClosed)
                {
                    throw new ObjectDisposedException(nameof(BusStats));
                }
                return _handle;
            }
        }

        public ulong Frames => NativeClient.autd3_bus_stats_frames(Handle);
        public ulong Resets => NativeClient.autd3_bus_stats_resets(Handle);
        public ulong Heartbeats => NativeClient.autd3_bus_stats_heartbeats(Handle);
        public ulong MissedReplies => NativeClient.autd3_bus_stats_missed_replies(Handle);
        public ulong AckedFrames => NativeClient.autd3_bus_stats_acked_frames(Handle);
        public ulong WorstAckLatencyNs => NativeClient.autd3_bus_stats_worst_ack_latency_ns(Handle);
        public ulong MeanAckLatencyNs => NativeClient.autd3_bus_stats_mean_ack_latency_ns(Handle);

        public void Dispose() => _handle.Dispose();

        public override string ToString() =>
            $"BusStats {{ Frames = {Frames}, Resets = {Resets}, Heartbeats = {Heartbeats}, MissedReplies = {MissedReplies}, " +
            $"AckedFrames = {AckedFrames}, WorstAckLatencyNs = {WorstAckLatencyNs}, MeanAckLatencyNs = {MeanAckLatencyNs} }}";
    }

    public static class TelemetryExt
    {
        public static readonly IReadOnlyList<Telemetry> All = ReadAll();

        private static Telemetry[] ReadAll()
        {
            var ids = new byte[(int)NativeClient.autd3_telemetry_count()];
            if (NativeClient.autd3_telemetry_all(ids, (UIntPtr)ids.Length) != 0)
            {
                throw new Autd3Exception("failed to read the telemetry counters");
            }
            return Array.ConvertAll(ids, id => (Telemetry)id);
        }
    }

    public readonly struct TelemetryCounters : IEquatable<TelemetryCounters>
    {
        public static readonly int Count = (int)NativeClient.autd3_telemetry_count();

        private readonly uint[] _counters;

        internal TelemetryCounters(uint[] counters)
        {
            _counters = counters;
        }

        public uint this[Telemetry counter] => Get(counter);

        public uint Get(Telemetry counter)
        {
            var index = (int)counter;
            return _counters != null && index < _counters.Length ? _counters[index] : 0;
        }

        public IReadOnlyList<uint> AsArray() => _counters ?? Array.Empty<uint>();

        public bool Equals(TelemetryCounters other)
        {
            var lhs = AsArray();
            var rhs = other.AsArray();
            if (lhs.Count != rhs.Count)
            {
                return false;
            }
            for (var i = 0; i < lhs.Count; i++)
            {
                if (lhs[i] != rhs[i])
                {
                    return false;
                }
            }
            return true;
        }

        public override bool Equals(object? obj) => obj is TelemetryCounters other && Equals(other);

        public override int GetHashCode()
        {
            var hash = new HashCode();
            foreach (var value in AsArray())
            {
                hash.Add(value);
            }
            return hash.ToHashCode();
        }

        public static bool operator ==(TelemetryCounters left, TelemetryCounters right) => left.Equals(right);

        public static bool operator !=(TelemetryCounters left, TelemetryCounters right) => !left.Equals(right);
    }

    public sealed class DeviceStatus
    {
        public IReadOnlyList<DeviceState> Devices { get; }

        internal DeviceStatus(IReadOnlyList<DeviceState> devices)
        {
            Devices = devices;
        }

        public bool AllReady
        {
            get
            {
                foreach (var state in Devices)
                {
                    if (state != DeviceState.Ready)
                    {
                        return false;
                    }
                }
                return true;
            }
        }

        public bool AnyLost
        {
            get
            {
                foreach (var state in Devices)
                {
                    if (state == DeviceState.Lost)
                    {
                        return true;
                    }
                }
                return false;
            }
        }

        public override bool Equals(object? obj)
        {
            if (obj is not DeviceStatus other)
            {
                return false;
            }
            if (Devices.Count != other.Devices.Count)
            {
                return false;
            }
            for (var i = 0; i < Devices.Count; i++)
            {
                if (Devices[i] != other.Devices[i])
                {
                    return false;
                }
            }
            return true;
        }

        public override int GetHashCode()
        {
            var hash = new HashCode();
            foreach (var state in Devices)
            {
                hash.Add(state);
            }
            return hash.ToHashCode();
        }

        public static bool operator ==(DeviceStatus? left, DeviceStatus? right) =>
            left is null ? right is null : left.Equals(right);

        public static bool operator !=(DeviceStatus? left, DeviceStatus? right) => !(left == right);
    }

    public sealed class StateChecker : IDisposable
    {
        private readonly CheckerHandle _handle;

        internal StateChecker(IntPtr handle)
        {
            _handle = new CheckerHandle(handle);
        }

        public DeviceStatus Check()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            var status = NativeClient.autd3_checker_check(_handle, out var code, err, (UIntPtr)err.Length);
            if (status == IntPtr.Zero)
            {
                throw Autd3Exception.FromNative(code, err);
            }
            try
            {
                var count = (int)NativeClient.autd3_device_status_num_devices(status);
                var devices = new List<DeviceState>(count);
                for (var i = 0; i < count; i++)
                {
                    if (!NativeClient.autd3_device_status_device_state(status, (UIntPtr)i, out var kind))
                    {
                        throw new Autd3Exception("failed to read device state");
                    }
                    devices.Add(DeviceState.FromNative(kind));
                }
                return new DeviceStatus(devices);
            }
            finally
            {
                NativeClient.autd3_device_status_free(status);
            }
        }

        public void Dispose() => _handle.Dispose();
    }

    public sealed class Response
    {
        private readonly byte[] _status;
        private readonly byte[][] _values;

        internal Response(byte[] status, byte[][] values)
        {
            _status = status;
            _values = values;
        }

        internal static Response FromNative(IntPtr handle)
        {
            try
            {
                var count = (int)NativeClient.autd3_response_num_devices(handle);
                var status = new byte[count];
                if (count > 0)
                {
                    Marshal.Copy(NativeClient.autd3_response_status(handle), status, 0, count);
                }
                var values = new byte[count][];
                for (var i = 0; i < count; i++)
                {
                    var len = (int)NativeClient.autd3_response_value_len(handle, (UIntPtr)i);
                    values[i] = new byte[len];
                    if (len > 0)
                    {
                        Marshal.Copy(NativeClient.autd3_response_value_data(handle, (UIntPtr)i), values[i], 0, len);
                    }
                }
                return new Response(status, values);
            }
            finally
            {
                NativeClient.autd3_response_free(handle);
            }
        }

        public IReadOnlyList<byte> Status => _status;

        public IReadOnlyList<IReadOnlyList<byte>> Values => _values;

        public IReadOnlyList<byte> Value(int device) =>
            device >= 0 && device < _values.Length ? _values[device] : Array.Empty<byte>();

        public void Check()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeClient.autd3_response_check(_status, (UIntPtr)_status.Length, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
        }
    }

    public sealed class ResponseFuture : IDisposable
    {
        private IntPtr _handle;

        internal ResponseFuture(IntPtr handle)
        {
            _handle = handle;
        }

        public async Task<Response> AwaitAsync()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("ResponseFuture has already been awaited");
            }
            GC.SuppressFinalize(this);
            var response = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_response_token_await(handle, cb, ud)).ConfigureAwait(false);
            return Response.FromNative(response);
        }

        public TaskAwaiter<Response> GetAwaiter() => AwaitAsync().GetAwaiter();

        private void Release()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_response_token_free(handle);
            }
        }

        public void Dispose()
        {
            Release();
            GC.SuppressFinalize(this);
        }

        ~ResponseFuture() => Release();
    }

    public sealed class StreamFuture : IDisposable
    {
        private IntPtr _handle;

        internal StreamFuture(IntPtr handle)
        {
            _handle = handle;
        }

        public async Task AwaitAsync()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("StreamFuture has already been awaited");
            }
            GC.SuppressFinalize(this);
            await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_stream_token_await(handle, cb, ud)).ConfigureAwait(false);
        }

        public TaskAwaiter GetAwaiter() => AwaitAsync().GetAwaiter();

        private void Release()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_stream_token_free(handle);
            }
        }

        public void Dispose()
        {
            Release();
            GC.SuppressFinalize(this);
        }

        ~StreamFuture() => Release();
    }

    public sealed class Client : IDisposable, IAsyncDisposable
    {
        public static readonly int MaxInflight = Params.MaxInflight;

        public static readonly int MaxDevices = (int)NativeClient.autd3_params_max_devices();

        private const long AllFrames = -1;

        private readonly ClientHandle _handle;
        private readonly Geometry _geometry;

        internal ClientHandle Handle => _handle;

        internal Client(IntPtr handle, Geometry geometry)
        {
            _handle = new ClientHandle(handle);
            _geometry = geometry;
        }

        public static async Task<Client> OpenAsync(Geometry geometry, TransportOption option, ClientConfig config)
        {
            var owned = geometry.Clone();
            IntPtr optionHandle;
            try
            {
                optionHandle = option.CreateHandle();
            }
            catch
            {
                owned.Dispose();
                throw;
            }

            Task<IntPtr> task;
            try
            {
                var configHandle = config.CreateHandle();
                try
                {
                    task = AsyncOps.InvokeAsync((cb, ud) =>
                        NativeClient.autd3_client_open(geometry.Handle, optionHandle, configHandle, cb, ud));
                }
                finally
                {
                    NativeClient.autd3_client_config_free(configHandle);
                }
            }
            catch
            {
                NativeClient.autd3_transport_option_free(optionHandle);
                owned.Dispose();
                throw;
            }
            IntPtr value;
            try
            {
                value = await task.ConfigureAwait(false);
            }
            catch
            {
                owned.Dispose();
                throw;
            }
            return new Client(value, owned);
        }

        public StateChecker StateChecker()
        {
            var checker = NativeClient.autd3_client_state_checker(Handle);
            if (checker == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create checker");
            }
            return new StateChecker(checker);
        }

        public BusStats BusStats()
        {
            var stats = NativeClient.autd3_client_bus_stats(Handle);
            if (stats == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to read the bus statistics");
            }
            return new BusStats(stats);
        }

        public int NumDevices => (int)NativeClient.autd3_client_num_devices(Handle);

        public Geometry Geometry => _geometry;

        public SysTime DeviceTimeNow()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeClient.autd3_client_device_time_now(Handle, out var ns, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
            return SysTime.FromNanos(ns);
        }

        public async Task SendAsync(ICommand command)
        {
            using var frames = Frames.Encode(_geometry, command);
            await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send_checked(Handle, frames.Handle, AllFrames, cb, ud)).ConfigureAwait(false);
        }

        public async Task<StreamFuture> SendStreamingAsync(ICommand command)
        {
            if (command == null)
            {
                throw new ArgumentNullException(nameof(command));
            }
            var op = Command.CreateOp(command, _geometry);
            Task<IntPtr> queued;
            try
            {
                queued = AsyncOps.InvokeAsync((cb, ud) =>
                    NativeClient.autd3_client_send_streaming(Handle, op, cb, ud));
            }
            catch
            {
                NativeClient.autd3_op_free(op);
                throw;
            }
            return new StreamFuture(await queued.ConfigureAwait(false));
        }

        public async Task<ResponseFuture> SendFrameAsync(Frame frame)
        {
            var token = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send(Handle, frame.Frames.Handle, frame.Index, cb, ud)).ConfigureAwait(false);
            return new ResponseFuture(token);
        }

        public async Task<IReadOnlyList<FirmwareVersion>> ReadFirmwareVersionAsync()
        {
            var array = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_read_firmware_version(Handle, cb, ud)).ConfigureAwait(false);
            try
            {
                var count = (int)NativeClient.autd3_firmware_version_array_len(array);
                var versions = new FirmwareVersion[count];
                for (var i = 0; i < count; i++)
                {
                    if (NativeClient.autd3_firmware_version_array_get(array, (UIntPtr)i, out var native) != 0)
                    {
                        throw new Autd3Exception("failed to read the firmware version");
                    }
                    versions[i] = new FirmwareVersion(native);
                }
                return versions;
            }
            finally
            {
                NativeClient.autd3_firmware_version_array_free(array);
            }
        }

        public async Task<IReadOnlyList<FpgaState>> ReadFpgaStateAsync()
        {
            var bytes = await ReadByteArrayAsync((cb, ud) =>
                NativeClient.autd3_client_read_fpga_state(Handle, cb, ud)).ConfigureAwait(false);
            var states = new FpgaState[bytes.Length];
            for (var i = 0; i < bytes.Length; i++)
            {
                states[i] = new FpgaState(bytes[i]);
            }
            return states;
        }

        public async Task<IReadOnlyList<TelemetryCounters>> ReadTelemetryAsync()
        {
            var array = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_read_telemetry(Handle, cb, ud)).ConfigureAwait(false);
            try
            {
                var len = (int)NativeClient.autd3_u32_array_len(array);
                var values = new int[len];
                if (len > 0)
                {
                    Marshal.Copy(NativeClient.autd3_u32_array_data(array), values, 0, len);
                }
                var devices = new TelemetryCounters[len / TelemetryCounters.Count];
                for (var d = 0; d < devices.Length; d++)
                {
                    var counters = new uint[TelemetryCounters.Count];
                    for (var i = 0; i < counters.Length; i++)
                    {
                        counters[i] = unchecked((uint)values[d * TelemetryCounters.Count + i]);
                    }
                    devices[d] = new TelemetryCounters(counters);
                }
                return devices;
            }
            finally
            {
                NativeClient.autd3_u32_array_free(array);
            }
        }

        internal static async Task<byte[]> ReadByteArrayAsync(Action<CompletionCallback, IntPtr> invoke)
        {
            var array = await AsyncOps.InvokeAsync(invoke).ConfigureAwait(false);
            try
            {
                var len = (int)NativeClient.autd3_byte_array_len(array);
                var bytes = new byte[len];
                if (len > 0)
                {
                    Marshal.Copy(NativeClient.autd3_byte_array_data(array), bytes, 0, len);
                }
                return bytes;
            }
            finally
            {
                NativeClient.autd3_byte_array_free(array);
            }
        }

        public Task SilentStopAsync() =>
            AsyncOps.InvokeAsync((cb, ud) => NativeClient.autd3_client_silent_stop(Handle, cb, ud));

        public Task CloseAsync() =>
            AsyncOps.InvokeAsync((cb, ud) => NativeClient.autd3_client_close(Handle, cb, ud));

        public void Dispose()
        {
            if (_handle.IsClosed)
            {
                return;
            }
            try
            {
                CloseAsync().GetAwaiter().GetResult();
            }
            finally
            {
                _handle.Dispose();
                _geometry.Dispose();
            }
        }

        public async ValueTask DisposeAsync()
        {
            if (_handle.IsClosed)
            {
                return;
            }
            try
            {
                await CloseAsync().ConfigureAwait(false);
            }
            finally
            {
                _handle.Dispose();
                _geometry.Dispose();
            }
        }
    }
}
