using System;
using System.Threading;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace AUTD3
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct ExecAtNative
    {
        public byte Kind;
        public ulong Ns;
    }

    public readonly struct ExecAt
    {
        internal byte Kind { get; }
        internal ulong Ns { get; }

        private ExecAt(byte kind, ulong ns)
        {
            Kind = kind;
            Ns = ns;
        }

        public static ExecAt Immediate => new ExecAt(0, 0);

        public static ExecAt Lead(TimeSpan lead) =>
            lead >= TimeSpan.Zero
                ? new ExecAt(1, OptionNative.ToNanos(lead))
                : throw new Autd3Exception("ExecAt.Lead must not be negative", Autd3ErrorCode.InvalidArgument);

        public static ExecAt At(SysTime sysTime) => new ExecAt(2, sysTime.Nanos);

        internal ExecAtNative ToNative() => new ExecAtNative { Kind = Kind, Ns = Ns };
    }

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
        public ulong Heartbeats => NativeClient.autd3_bus_stats_heartbeats(Handle);
        public ulong AckedFrames => NativeClient.autd3_bus_stats_acked_frames(Handle);
        public ulong WorstAckLatencyNs => NativeClient.autd3_bus_stats_worst_ack_latency_ns(Handle);
        public ulong MeanAckLatencyNs => NativeClient.autd3_bus_stats_mean_ack_latency_ns(Handle);

        public void Dispose() => _handle.Dispose();

        public override string ToString() =>
            $"BusStats {{ Frames = {Frames}, Heartbeats = {Heartbeats}, " +
            $"AckedFrames = {AckedFrames}, WorstAckLatencyNs = {WorstAckLatencyNs}, MeanAckLatencyNs = {MeanAckLatencyNs} }}";
    }

    public readonly struct Telemetry : IEquatable<Telemetry>
    {
        public uint FifoDrop { get; }
        public uint DispatchError { get; }
        public uint Processed { get; }
        public uint SendFailure { get; }
        public ushort Failsafe { get; }
        public ushort PtpUnlockFailsafe { get; }
        public byte SyncResync { get; }
        public byte BootFailure { get; }
        public byte CpuWatchdog { get; }
        public uint ExecLate { get; }

        internal Telemetry(NativeClient.TelemetryNative native)
        {
            FifoDrop = native.FifoDrop;
            DispatchError = native.DispatchError;
            Processed = native.Processed;
            SendFailure = native.SendFailure;
            Failsafe = native.Failsafe;
            PtpUnlockFailsafe = native.PtpUnlockFailsafe;
            SyncResync = native.SyncResync;
            BootFailure = native.BootFailure;
            CpuWatchdog = native.CpuWatchdog;
            ExecLate = native.ExecLate;
        }

        public override string ToString() =>
            $"Telemetry {{ FifoDrop = {FifoDrop}, DispatchError = {DispatchError}, " +
            $"Processed = {Processed}, SendFailure = {SendFailure}, Failsafe = {Failsafe}, PtpUnlockFailsafe = {PtpUnlockFailsafe}, " +
            $"SyncResync = {SyncResync}, BootFailure = {BootFailure}, CpuWatchdog = {CpuWatchdog}, ExecLate = {ExecLate} }}";

        public bool Equals(Telemetry other) =>
            FifoDrop == other.FifoDrop &&
            DispatchError == other.DispatchError && Processed == other.Processed && SendFailure == other.SendFailure &&
            Failsafe == other.Failsafe && PtpUnlockFailsafe == other.PtpUnlockFailsafe &&
            SyncResync == other.SyncResync && BootFailure == other.BootFailure && CpuWatchdog == other.CpuWatchdog &&
            ExecLate == other.ExecLate;

        public override bool Equals(object? obj) => obj is Telemetry other && Equals(other);

        public override int GetHashCode()
        {
            var hash = new HashCode();
            hash.Add(FifoDrop);
            hash.Add(DispatchError);
            hash.Add(Processed);
            hash.Add(SendFailure);
            hash.Add(Failsafe);
            hash.Add(PtpUnlockFailsafe);
            hash.Add(SyncResync);
            hash.Add(BootFailure);
            hash.Add(CpuWatchdog);
            hash.Add(ExecLate);
            return hash.ToHashCode();
        }

        public static bool operator ==(Telemetry left, Telemetry right) => left.Equals(right);

        public static bool operator !=(Telemetry left, Telemetry right) => !left.Equals(right);
    }

    public sealed class DeviceStatus
    {
        public IReadOnlyList<DeviceState> Devices { get; }

        public DeviceMask PtpLocked { get; }

        public DeviceMask PtpFailsafe { get; }

        internal DeviceStatus(IReadOnlyList<DeviceState> devices, DeviceMask ptpLocked, DeviceMask ptpFailsafe)
        {
            Devices = devices;
            PtpLocked = ptpLocked;
            PtpFailsafe = ptpFailsafe;
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

        public bool AnyPtpFailsafe => !PtpFailsafe.IsEmpty;

        public override bool Equals(object? obj)
        {
            if (obj is not DeviceStatus other)
            {
                return false;
            }
            if (Devices.Count != other.Devices.Count || PtpLocked != other.PtpLocked || PtpFailsafe != other.PtpFailsafe)
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
            hash.Add(PtpLocked);
            hash.Add(PtpFailsafe);
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
                var ptpLocked = new List<int>();
                var ptpFailsafe = new List<int>();
                for (var i = 0; i < count; i++)
                {
                    if (!NativeClient.autd3_device_status_device_state(status, (UIntPtr)i, out var kind))
                    {
                        throw new Autd3Exception("failed to read device state");
                    }
                    devices.Add(DeviceState.FromNative(kind));
                    if (NativeClient.autd3_device_status_ptp_locked(status, (UIntPtr)i))
                    {
                        ptpLocked.Add(i);
                    }
                    if (NativeClient.autd3_device_status_ptp_failsafe(status, (UIntPtr)i))
                    {
                        ptpFailsafe.Add(i);
                    }
                }
                return new DeviceStatus(devices, DeviceMask.FromDevices(ptpLocked), DeviceMask.FromDevices(ptpFailsafe));
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
        private readonly byte[] _errorCodes;
        private readonly byte[][] _replyData;

        internal Response(byte[] errorCodes, byte[][] replyData)
        {
            _errorCodes = errorCodes;
            _replyData = replyData;
        }

        internal static Response FromNative(IntPtr handle)
        {
            try
            {
                var count = (int)NativeClient.autd3_response_num_devices(handle);
                var errorCodes = new byte[count];
                if (count > 0)
                {
                    Marshal.Copy(NativeClient.autd3_response_error_codes(handle), errorCodes, 0, count);
                }
                var replyData = new byte[count][];
                for (var i = 0; i < count; i++)
                {
                    var len = (int)NativeClient.autd3_response_reply_data_of_len(handle, (UIntPtr)i);
                    replyData[i] = new byte[len];
                    if (len > 0)
                    {
                        Marshal.Copy(NativeClient.autd3_response_reply_data_of(handle, (UIntPtr)i), replyData[i], 0, len);
                    }
                }
                return new Response(errorCodes, replyData);
            }
            finally
            {
                NativeClient.autd3_response_free(handle);
            }
        }

        public IReadOnlyList<byte> ErrorCodes => _errorCodes;

        public IReadOnlyList<IReadOnlyList<byte>> ReplyData => _replyData;

        public IReadOnlyList<byte> ReplyDataOf(int device) =>
            device >= 0 && device < _replyData.Length ? _replyData[device] : Array.Empty<byte>();

        public void Check()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeClient.autd3_response_check(_errorCodes, (UIntPtr)_errorCodes.Length, err, (UIntPtr)err.Length);
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

    public sealed class Client : IDisposable, IAsyncDisposable
    {
        public static readonly int MaxInflight = Params.MaxInflight;

        public static readonly int MaxDevices = (int)NativeClient.autd3_params_max_devices();

        private const long AllFrames = -1;

        private readonly ClientHandle _handle;
        private readonly Geometry _geometry;
        private volatile bool _closed;

        internal ClientHandle Handle => _handle;

        internal Client(IntPtr handle, Geometry geometry)
        {
            _handle = new ClientHandle(handle);
            _geometry = geometry;
        }

        public static Client Open(Geometry geometry, TransportOption option)
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

            var err = new byte[NativeAbi.ErrorBufferLength];
            int code;
            IntPtr value;
            try
            {
                code = NativeClient.autd3_client_open(geometry.Handle, optionHandle, out value, err, (UIntPtr)err.Length);
            }
            catch
            {
                NativeClient.autd3_transport_option_free(optionHandle);
                owned.Dispose();
                throw;
            }
            if (code != 0)
            {
                owned.Dispose();
                throw Autd3Exception.FromNative(code, err);
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

        public void SendHeartbeat()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeClient.autd3_client_send_heartbeat(Handle, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
        }

        public async Task SendAsync(ICommand command)
        {
            using var frames = Frames.Encode(_geometry, command);
            await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send_checked(Handle, frames.Handle, AllFrames, cb, ud)).ConfigureAwait(false);
        }

        public async Task<ResponseFuture> SendFrameAsync(Frame frame)
        {
            var token = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send(Handle, frame.Frames.Handle, frame.Index, cb, ud)).ConfigureAwait(false);
            return new ResponseFuture(token);
        }

        public async Task<ResponseFuture> SendFrameAsync(Frame frame, ExecAt execAt)
        {
            var exec = execAt.ToNative();
            var token = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send_at(Handle, frame.Frames.Handle, frame.Index, exec, cb, ud)).ConfigureAwait(false);
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

        public async Task<IReadOnlyList<Telemetry>> ReadTelemetryAsync()
        {
            var array = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_read_telemetry(Handle, cb, ud)).ConfigureAwait(false);
            try
            {
                var count = (int)NativeClient.autd3_telemetry_array_len(array);
                var telemetry = new Telemetry[count];
                for (var i = 0; i < count; i++)
                {
                    if (NativeClient.autd3_telemetry_array_get(array, (UIntPtr)i, out var native) != 0)
                    {
                        throw new Autd3Exception("failed to read the telemetry");
                    }
                    telemetry[i] = new Telemetry(native);
                }
                return telemetry;
            }
            finally
            {
                NativeClient.autd3_telemetry_array_free(array);
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

        public async Task CloseAsync()
        {
            try
            {
                await AsyncOps.InvokeAsync((cb, ud) => NativeClient.autd3_client_close(Handle, cb, ud)).ConfigureAwait(false);
            }
            finally
            {
                _closed = true;
            }
        }

        public void Dispose()
        {
            if (_handle.IsClosed)
            {
                return;
            }
            try
            {
                if (!_closed)
                {
                    CloseAsync().GetAwaiter().GetResult();
                }
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
                if (!_closed)
                {
                    await CloseAsync().ConfigureAwait(false);
                }
            }
            finally
            {
                _handle.Dispose();
                _geometry.Dispose();
            }
        }
    }
}
