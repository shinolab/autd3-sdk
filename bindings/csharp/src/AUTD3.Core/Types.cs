using System;
using System.Numerics;

namespace AUTD3
{
    public enum Autd3ErrorCode
    {
        Error = -1,
        Timeout = -2,
        Device = -3,
        Network = -4,
        InvalidArgument = -5,
        Aborted = -7,
    }

    public sealed class Autd3Exception : Exception
    {
        public Autd3ErrorCode Code { get; }

        public Autd3Exception(string message) : this(message, Autd3ErrorCode.Error)
        {
        }

        public Autd3Exception(string message, Autd3ErrorCode code) : base(message)
        {
            Code = code;
        }

        internal static Autd3Exception FromNative(int code, string message) =>
            new Autd3Exception(message, Enum.IsDefined(typeof(Autd3ErrorCode), code) ? (Autd3ErrorCode)code : Autd3ErrorCode.Error);

        internal static Autd3Exception FromNative(int code, byte[] err) => FromNative(code, NativeUtil.Utf8(err));
    }

    public static class Params
    {
        public static readonly TimeSpan UltrasoundPeriod = OptionNative.FromNanos(NativeCore.autd3_core_params_ultrasound_period_ns());
        public static readonly uint UltrasoundFreqHz = NativeCore.autd3_core_params_ultrasound_freq_hz();
        public static readonly int ModBufferSamples = (int)NativeCore.autd3_core_params_mod_buffer_samples();
        public static readonly int BufferSizeMin = (int)NativeCore.autd3_core_params_buffer_size_min();
        public static readonly int EmissionMaxIndices = (int)NativeCore.autd3_core_params_emission_max_indices();
        public static readonly byte NumFociMax = NativeCore.autd3_core_params_num_foci_max();
        public static readonly ushort PulseWidthPeriod = NativeCore.autd3_core_params_pulse_width_period();
        public static readonly int MaxInflight = (int)NativeCore.autd3_core_params_max_inflight();
        public static readonly int MaxDevices = (int)NativeCore.autd3_core_params_max_devices();
        public static readonly ushort Port = NativeCore.autd3_core_params_port();
        public static readonly int DeviceQueueFrames = (int)NativeCore.autd3_core_params_device_queue_frames();
    }

    public readonly struct Autd3 : IEquatable<Autd3>
    {
        public static readonly int NumTransducers = (int)NativeCore.autd3_core_params_num_transducers();
        public static readonly uint GridX = NativeCore.autd3_core_params_grid_x();
        public static readonly uint GridY = NativeCore.autd3_core_params_grid_y();
        public static readonly float PitchMm = NativeCore.autd3_core_params_pitch_mm();
        public static readonly float DeviceWidth = NativeCore.autd3_core_params_device_width_mm();
        public static readonly float DeviceHeight = NativeCore.autd3_core_params_device_height_mm();
        public static readonly float RotationNormTolerance = NativeCore.autd3_core_params_rotation_norm_tolerance();

        private readonly Quaternion? _rotation;

        public Vector3 Origin { get; }
        public Quaternion Rotation => _rotation ?? Coords.IdentityRotation;

        public Autd3(Vector3 origin)
        {
            Origin = origin;
            _rotation = null;
        }

        public Autd3(Vector3 origin, Quaternion rotation)
        {
            Origin = origin;
            _rotation = rotation;
        }

        public bool Equals(Autd3 other) => Origin.Equals(other.Origin) && Rotation.Equals(other.Rotation);

        public override bool Equals(object? obj) => obj is Autd3 other && Equals(other);

        public override int GetHashCode() => HashCode.Combine(Origin, Rotation);

        public static bool operator ==(Autd3 left, Autd3 right) => left.Equals(right);

        public static bool operator !=(Autd3 left, Autd3 right) => !left.Equals(right);

        internal NativeCore.Autd3Device ToNative()
        {
            var o = Coords.Point(Origin);
            var r = Coords.Rotation(Rotation);
            return new NativeCore.Autd3Device
            {
                Ox = o.X,
                Oy = o.Y,
                Oz = o.Z,
                Rw = r.W,
                Rx = r.X,
                Ry = r.Y,
                Rz = r.Z,
            };
        }
    }

    public readonly struct Intensity : IEquatable<Intensity>, IComparable<Intensity>
    {
        private readonly byte _inverted;

        public byte Value => (byte)~_inverted;

        public Intensity(byte value)
        {
            _inverted = (byte)~value;
        }

        public bool Equals(Intensity other) => _inverted == other._inverted;

        public override bool Equals(object? obj) => obj is Intensity other && Equals(other);

        public override int GetHashCode() => Value;

        public int CompareTo(Intensity other) => Value.CompareTo(other.Value);

        public static bool operator ==(Intensity left, Intensity right) => left.Equals(right);

        public static bool operator !=(Intensity left, Intensity right) => !left.Equals(right);

        public static bool operator <(Intensity left, Intensity right) => left.Value < right.Value;

        public static bool operator >(Intensity left, Intensity right) => left.Value > right.Value;

        public static bool operator <=(Intensity left, Intensity right) => left.Value <= right.Value;

        public static bool operator >=(Intensity left, Intensity right) => left.Value >= right.Value;

        public static Intensity Max => new Intensity(0xFF);
        public static Intensity Min => new Intensity(0x00);

        public static Intensity operator +(Intensity lhs, Intensity rhs) =>
            new Intensity((byte)Math.Min(lhs.Value + rhs.Value, 0xFF));

        public static Intensity operator -(Intensity lhs, Intensity rhs) =>
            new Intensity((byte)Math.Max(lhs.Value - rhs.Value, 0x00));

        public static Intensity operator *(Intensity lhs, byte rhs) =>
            new Intensity((byte)Math.Min(lhs.Value * rhs, 0xFF));

        public static Intensity operator *(byte lhs, Intensity rhs) => rhs * lhs;

        public static Intensity operator /(Intensity lhs, byte rhs) =>
            new Intensity((byte)(lhs.Value / rhs));
    }

    public readonly struct Phase : IEquatable<Phase>
    {
        public byte Value { get; }

        public Phase(byte value)
        {
            Value = value;
        }

        public bool Equals(Phase other) => Value == other.Value;

        public override bool Equals(object? obj) => obj is Phase other && Equals(other);

        public override int GetHashCode() => Value;

        public static bool operator ==(Phase left, Phase right) => left.Equals(right);

        public static bool operator !=(Phase left, Phase right) => !left.Equals(right);

        public static Phase Zero => new Phase(0x00);
        public static Phase Pi => new Phase(0x80);

        public float Rad() => Value / 256f * 2f * MathF.PI;

        public static explicit operator Phase(Angle angle) =>
            new Phase(NativeCore.autd3_core_phase_from_rad(angle.Rad));

        public static explicit operator Phase(Complex value) =>
            (Phase)new Angle(MathF.Atan2((float)value.Imaginary, (float)value.Real));

        public static Phase operator +(Phase lhs, Phase rhs) =>
            new Phase(unchecked((byte)(lhs.Value + rhs.Value)));

        public static Phase operator -(Phase lhs, Phase rhs) =>
            new Phase(unchecked((byte)(lhs.Value - rhs.Value)));

        public static Phase operator *(Phase lhs, byte rhs) =>
            new Phase(unchecked((byte)(lhs.Value * rhs)));

        public static Phase operator *(byte lhs, Phase rhs) => rhs * lhs;

        public static Phase operator /(Phase lhs, byte rhs) =>
            new Phase((byte)(lhs.Value / rhs));
    }

    public readonly struct Interface : IEquatable<Interface>
    {
        private const byte KindNic = 0;
        private const byte KindAddr = 1;

        private readonly byte _kind;
        private readonly string? _value;

        private Interface(byte kind, string? value)
        {
            _kind = kind;
            _value = value;
        }

        public static Interface Auto => default;

        public static Interface Name(string name) => new Interface(KindNic, name);

        public static Interface Addr(string addr) => new Interface(KindAddr, addr);

        public string? NameValue => _kind == KindNic ? _value : null;

        public bool IsAuto => _kind == KindNic && _value == null;

        public string? AddrValue => _kind == KindAddr ? _value : null;

        public bool Equals(Interface other) => _kind == other._kind && string.Equals(_value, other._value, StringComparison.Ordinal);

        public override bool Equals(object? obj) => obj is Interface other && Equals(other);

        public override int GetHashCode() => HashCode.Combine(_kind, _value is null ? 0 : StringComparer.Ordinal.GetHashCode(_value));

        public static bool operator ==(Interface left, Interface right) => left.Equals(right);

        public static bool operator !=(Interface left, Interface right) => !left.Equals(right);
    }

    public readonly struct DeviceState : IEquatable<DeviceState>
    {
        private readonly byte _kind;

        private DeviceState(byte kind)
        {
            _kind = kind;
        }

        public static DeviceState Ready => new DeviceState(0);
        public static DeviceState Syncing => new DeviceState(1);
        public static DeviceState Lost => new DeviceState(2);

        internal static DeviceState FromNative(byte kind) => new DeviceState(kind);

        public override string ToString() => _kind switch
        {
            0 => "READY",
            1 => "SYNCING",
            2 => "LOST",
            _ => $"UNKNOWN ({_kind})",
        };

        public bool Equals(DeviceState other) => _kind == other._kind;

        public override bool Equals(object? obj) => obj is DeviceState other && Equals(other);

        public override int GetHashCode() => _kind;

        public static bool operator ==(DeviceState left, DeviceState right) => left.Equals(right);

        public static bool operator !=(DeviceState left, DeviceState right) => !left.Equals(right);
    }

    public readonly struct SysTime : IEquatable<SysTime>, IComparable<SysTime>
    {
        private readonly ulong _ns;

        private SysTime(ulong ns)
        {
            _ns = ns;
        }

        public static SysTime Zero => new SysTime(0);

        public static SysTime FromNanos(ulong ns) => new SysTime(ns);

        public ulong Nanos => _ns;

        public static SysTime operator +(SysTime lhs, TimeSpan rhs)
        {
            if (rhs < TimeSpan.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(rhs));
            }
            var ns = OptionNative.ToNanos(rhs);
            return new SysTime(ns > ulong.MaxValue - lhs._ns ? ulong.MaxValue : lhs._ns + ns);
        }

        public static SysTime operator -(SysTime lhs, TimeSpan rhs)
        {
            if (rhs < TimeSpan.Zero)
            {
                throw new ArgumentOutOfRangeException(nameof(rhs));
            }
            var ns = OptionNative.ToNanos(rhs);
            return new SysTime(ns > lhs._ns ? 0 : lhs._ns - ns);
        }

        public static TimeSpan operator -(SysTime lhs, SysTime rhs) =>
            OptionNative.FromNanos(rhs._ns > lhs._ns ? 0 : lhs._ns - rhs._ns);

        public bool Equals(SysTime other) => _ns == other._ns;

        public override bool Equals(object? obj) => obj is SysTime other && Equals(other);

        public override int GetHashCode() => _ns.GetHashCode();

        public int CompareTo(SysTime other) => _ns.CompareTo(other._ns);

        public static bool operator ==(SysTime left, SysTime right) => left.Equals(right);

        public static bool operator !=(SysTime left, SysTime right) => !left.Equals(right);

        public static bool operator <(SysTime left, SysTime right) => left._ns < right._ns;

        public static bool operator >(SysTime left, SysTime right) => left._ns > right._ns;

        public static bool operator <=(SysTime left, SysTime right) => left._ns <= right._ns;

        public static bool operator >=(SysTime left, SysTime right) => left._ns >= right._ns;
    }
}
