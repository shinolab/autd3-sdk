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
        public bool IsPatternStopped => NativeClient.autd3_fpga_state_is_pattern_stopped(Raw);
        public bool IsModStopped => NativeClient.autd3_fpga_state_is_mod_stopped(Raw);
        public bool IsTransitionPending => NativeClient.autd3_fpga_state_is_transition_pending(Raw);
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

    public readonly struct Telemetry : IEquatable<Telemetry>
    {
        public uint FifoDrop { get; }
        public uint ExecLate { get; }
        public byte SyncResync { get; }
        public byte CpuWatchdog { get; }
        public byte BootFailure { get; }

        internal Telemetry(NativeClient.TelemetryNative native)
        {
            FifoDrop = native.FifoDrop;
            ExecLate = native.ExecLate;
            SyncResync = native.SyncResync;
            CpuWatchdog = native.CpuWatchdog;
            BootFailure = native.BootFailure;
        }

        public override string ToString() =>
            $"Telemetry {{ FifoDrop = {FifoDrop}, ExecLate = {ExecLate}, SyncResync = {SyncResync}, CpuWatchdog = {CpuWatchdog}, BootFailure = {BootFailure} }}";

        public bool Equals(Telemetry other) =>
            FifoDrop == other.FifoDrop && ExecLate == other.ExecLate &&
            SyncResync == other.SyncResync && CpuWatchdog == other.CpuWatchdog && BootFailure == other.BootFailure;

        public override bool Equals(object? obj) => obj is Telemetry other && Equals(other);

        public override int GetHashCode() => HashCode.Combine(FifoDrop, ExecLate, SyncResync, CpuWatchdog, BootFailure);

        public static bool operator ==(Telemetry left, Telemetry right) => left.Equals(right);

        public static bool operator !=(Telemetry left, Telemetry right) => !left.Equals(right);
    }

    public readonly struct DeviceStatus : IEquatable<DeviceStatus>
    {
        public DeviceState State { get; }

        public bool PtpLocked { get; }

        public bool FailsafeActive { get; }

        internal DeviceStatus(NativeClient.DeviceStatusNative native)
        {
            State = DeviceState.FromNative(native.State);
            PtpLocked = native.PtpLocked != 0;
            FailsafeActive = native.FailsafeActive != 0;
        }

        public override string ToString() =>
            $"DeviceStatus {{ State = {State}, PtpLocked = {PtpLocked}, FailsafeActive = {FailsafeActive} }}";

        public bool Equals(DeviceStatus other) =>
            State == other.State && PtpLocked == other.PtpLocked && FailsafeActive == other.FailsafeActive;

        public override bool Equals(object? obj) => obj is DeviceStatus other && Equals(other);

        public override int GetHashCode() => HashCode.Combine(State, PtpLocked, FailsafeActive);

        public static bool operator ==(DeviceStatus left, DeviceStatus right) => left.Equals(right);

        public static bool operator !=(DeviceStatus left, DeviceStatus right) => !left.Equals(right);
    }

    public sealed class DeviceReply
    {
        public int Device { get; }
        public ushort MsgId { get; }
        public byte Status { get; }
        public byte Flags { get; }
        public byte UnitId { get; }
        public ulong SysTime { get; }
        public Telemetry Telemetry { get; }
        public FpgaState FpgaState { get; }
        public IReadOnlyList<byte> Data { get; }

        private DeviceReply(NativeClient.DeviceReplyNative native, byte[] data)
        {
            Device = (int)native.Device;
            MsgId = native.MsgId;
            Status = native.Status;
            Flags = native.Flags;
            UnitId = native.UnitId;
            SysTime = native.SysTime;
            Telemetry = new Telemetry(native.Telemetry);
            FpgaState = new FpgaState(native.FpgaState);
            Data = data;
        }

        internal static DeviceReply FromNative(IntPtr reply)
        {
            if (NativeClient.autd3_device_reply_get(reply, out var native) != 0)
            {
                throw new Autd3Exception("failed to read the reply");
            }
            var data = new byte[(int)NativeClient.autd3_device_reply_data_len(reply)];
            if (data.Length > 0)
            {
                Marshal.Copy(NativeClient.autd3_device_reply_data(reply), data, 0, data.Length);
            }
            return new DeviceReply(native, data);
        }

        internal static DeviceReply FromOwned(IntPtr reply)
        {
            try
            {
                return FromNative(reply);
            }
            finally
            {
                NativeClient.autd3_device_reply_free(reply);
            }
        }
    }

    public sealed class Response
    {
        private readonly DeviceReply[] _replies;
        private readonly DeviceStatus[] _deviceStatus;
        private readonly int _checkCode;
        private readonly byte[] _checkError;

        private Response(IntPtr handle)
        {
            var count = (int)NativeClient.autd3_response_num_devices(handle);
            _replies = new DeviceReply[count];
            for (var i = 0; i < count; i++)
            {
                _replies[i] = DeviceReply.FromNative(NativeClient.autd3_response_reply(handle, (UIntPtr)i));
            }
            var native = new NativeClient.DeviceStatusNative[count];
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeClient.autd3_response_device_status(handle, native, (UIntPtr)native.Length, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
            _deviceStatus = new DeviceStatus[count];
            for (var i = 0; i < count; i++)
            {
                _deviceStatus[i] = new DeviceStatus(native[i]);
            }
            _checkError = new byte[NativeAbi.ErrorBufferLength];
            _checkCode = NativeClient.autd3_response_check(handle, _checkError, (UIntPtr)_checkError.Length);
        }

        internal static Response FromOwned(IntPtr handle)
        {
            try
            {
                return new Response(handle);
            }
            finally
            {
                NativeClient.autd3_response_free(handle);
            }
        }

        public IReadOnlyList<DeviceReply> Replies => _replies;

        public DeviceStatus[] DeviceStatus() => (DeviceStatus[])_deviceStatus.Clone();

        public void Check()
        {
            if (_checkCode != 0)
            {
                throw Autd3Exception.FromNative(_checkCode, _checkError);
            }
        }
    }

    public sealed class InflightFrames : IDisposable
    {
        private readonly InflightFramesHandle _handle = new InflightFramesHandle(NativeClient.autd3_inflight_frames_new());

        private static Autd3Exception Busy() => new Autd3Exception("the inflight frames are being collected");

        public void Push(ushort msgId)
        {
            switch (NativeClient.autd3_inflight_frames_push(_handle, msgId))
            {
                case 0:
                    return;
                case (int)Autd3ErrorCode.Error:
                    throw Busy();
                case var code:
                    throw new Autd3Exception("failed to push the frame", (Autd3ErrorCode)code);
            }
        }

        public int Length
        {
            get
            {
                switch (NativeClient.autd3_inflight_frames_len(_handle, out var len))
                {
                    case 0:
                        return (int)len;
                    case (int)Autd3ErrorCode.Error:
                        throw Busy();
                    case var code:
                        throw new Autd3Exception("failed to read the length", (Autd3ErrorCode)code);
                }
            }
        }

        public bool IsEmpty => Length == 0;

        public async Task<Response?> CollectOldestAsync(Client client, CancellationToken cancellationToken = default)
        {
            var response = await AsyncOps.InvokeAsync((cancel, cb, ud) =>
                NativeClient.autd3_inflight_frames_collect_oldest(_handle, client.Handle, cancel, cb, ud), cancellationToken).ConfigureAwait(false);
            return response == IntPtr.Zero ? null : Response.FromOwned(response);
        }

        public void Dispose() => _handle.Dispose();
    }

    public sealed class Client : IDisposable
    {
        private readonly ClientHandle _handle;
        private readonly Geometry _geometry;

        internal ClientHandle Handle => _handle;

        internal Client(IntPtr handle, Geometry geometry)
        {
            _handle = new ClientHandle(handle);
            _geometry = geometry;
        }

        public static Client Open(Geometry geometry, TransportOption option)
        {
            var native = option.ToNative();
            var owned = geometry.Clone();
            var err = new byte[NativeAbi.ErrorBufferLength];
            int code;
            IntPtr value;
            try
            {
                code = NativeClient.autd3_client_open(geometry.Handle, option.IfaceKind, option.IfaceValue, native, out value, err, (UIntPtr)err.Length);
            }
            catch
            {
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

        public void DiscardUnreadReplies()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeClient.autd3_client_discard_unread_replies(Handle, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
        }

        public ushort SendHeartbeat()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeClient.autd3_client_send_heartbeat(Handle, out var msgId, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
            return msgId;
        }

        public async Task SendAsync(ICommand command, CancellationToken cancellationToken = default)
        {
            using var frames = Frames.Encode(_geometry, command);
            await AsyncOps.InvokeAsync((cancel, cb, ud) =>
                NativeClient.autd3_client_send(Handle, frames.Handle, cancel, cb, ud), cancellationToken).ConfigureAwait(false);
        }

        public async Task<ushort> SendFrameAsync(Frame frame, ExecAt execAt, CancellationToken cancellationToken = default)
        {
            var exec = execAt.ToNative();
            var msgId = await AsyncOps.InvokeAsync((cancel, cb, ud) =>
                NativeClient.autd3_client_send_frame(Handle, frame.Frames.Handle, frame.Index, exec, cancel, cb, ud), cancellationToken).ConfigureAwait(false);
            return (ushort)msgId.ToInt64();
        }

        public async Task<DeviceReply> RecvAsync(CancellationToken cancellationToken = default)
        {
            var reply = await AsyncOps.InvokeAsync((cancel, cb, ud) =>
                NativeClient.autd3_client_recv(Handle, cancel, cb, ud), cancellationToken).ConfigureAwait(false);
            return DeviceReply.FromOwned(reply);
        }

        public async Task<Response> CollectAsync(ushort msgId, CancellationToken cancellationToken = default)
        {
            var response = await AsyncOps.InvokeAsync((cancel, cb, ud) =>
                NativeClient.autd3_client_collect(Handle, msgId, cancel, cb, ud), cancellationToken).ConfigureAwait(false);
            return Response.FromOwned(response);
        }

        public async Task<IReadOnlyList<FirmwareVersion>> ReadFirmwareVersionAsync(CancellationToken cancellationToken = default)
        {
            var array = await AsyncOps.InvokeAsync((cancel, cb, ud) =>
                NativeClient.autd3_client_read_firmware_version(Handle, cancel, cb, ud), cancellationToken).ConfigureAwait(false);
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

        public void Dispose()
        {
            if (_handle.IsClosed)
            {
                return;
            }
            _handle.Dispose();
            _geometry.Dispose();
        }
    }
}
