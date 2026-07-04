using System;
using System.Numerics;

namespace AUTD3
{
    public sealed class Autd3Exception : Exception
    {
        public Autd3Exception(string message) : base(message)
        {
        }
    }


    public readonly struct Autd3
    {
        public const int NumTransducers = 249;
        public const uint GridX = 18;
        public const uint GridY = 14;
        public const float PitchMm = 10.16f;
        public const float DeviceWidth = 192.0f;
        public const float DeviceHeight = 151.4f;

        public Vector3 Origin { get; }
        public Quaternion Rotation { get; }

        public Autd3(Vector3 origin) : this(origin, Quaternion.Identity)
        {
        }

        public Autd3(Vector3 origin, Quaternion rotation)
        {
            Origin = origin;
            Rotation = rotation;
        }

        internal NativeCore.Autd3Device ToNative()
        {
            return new NativeCore.Autd3Device
            {
                Ox = Origin.X,
                Oy = Origin.Y,
                Oz = Origin.Z,
                Rw = Rotation.W,
                Rx = Rotation.X,
                Ry = Rotation.Y,
                Rz = Rotation.Z,
            };
        }
    }

    public readonly struct Intensity
    {
        public byte Value { get; }

        public Intensity(byte value)
        {
            Value = value;
        }

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

    public readonly struct Phase
    {
        public byte Value { get; }

        public Phase(byte value)
        {
            Value = value;
        }

        public static Phase Zero => new Phase(0x00);
        public static Phase Pi => new Phase(0x80);

        public float Radian() => NativeCore.autd3_core_phase_radian(Value);

        public static explicit operator Phase(Angle angle)
        {
            var p = (int)MathF.Round(angle.Radian / (2f * MathF.PI) * 256f);
            return new Phase((byte)(p & 0xFF));
        }

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

    public readonly struct Emission
    {
        public Phase Phase { get; }
        public Intensity Intensity { get; }

        public Emission(Phase phase, Intensity intensity)
        {
            Phase = phase;
            Intensity = intensity;
        }

        public static Emission Null => new Emission(Phase.Zero, Intensity.Min);
    }

    public readonly struct Interface
    {
        private readonly string? _name;

        private Interface(string? name)
        {
            _name = name;
        }

        public static Interface Auto => default;

        public static Interface Name(string name) => new Interface(name);

        internal string? NameValue => _name;
    }

    public readonly struct DeviceState : IEquatable<DeviceState>
    {
        private readonly byte _kind;
        private readonly byte _bits;

        private DeviceState(byte kind, byte bits)
        {
            _kind = kind;
            _bits = bits;
        }

        public static DeviceState Op => new DeviceState(0, 0);
        public static DeviceState SafeOp => new DeviceState(1, 0);
        public static DeviceState SafeOpError => new DeviceState(2, 0);
        public static DeviceState Lost => new DeviceState(3, 0);
        public static DeviceState Other(byte bits) => new DeviceState(4, bits);

        internal static DeviceState FromNative(byte kind, byte bits) => new DeviceState(kind, bits);

        public override string ToString() => _kind switch
        {
            0 => "OP",
            1 => "SAFE-OP",
            2 => "SAFE-OP + ERROR",
            3 => "LOST",
            _ => _bits switch
            {
                0x00 => "NONE",
                0x01 => "INIT",
                0x02 => "PRE-OP",
                0x03 => "BOOT",
                _ => $"UNKNOWN (0x{_bits:x2})",
            },
        };

        public bool Equals(DeviceState other) => _kind == other._kind && _bits == other._bits;

        public override bool Equals(object? obj) => obj is DeviceState other && Equals(other);

        public override int GetHashCode() => (_kind << 8) | _bits;

        public static bool operator ==(DeviceState left, DeviceState right) => left.Equals(right);

        public static bool operator !=(DeviceState left, DeviceState right) => !left.Equals(right);
    }

    public readonly struct DcSysTime : IEquatable<DcSysTime>, IComparable<DcSysTime>
    {
        private static readonly DateTime EcatEpoch = new DateTime(2000, 1, 1, 0, 0, 0, DateTimeKind.Utc);

        private readonly ulong _ns;

        private DcSysTime(ulong ns)
        {
            _ns = ns;
        }

        public static DcSysTime Zero => new DcSysTime(0);

        public static DcSysTime FromNanos(ulong ns) => new DcSysTime(ns);

        public ulong SysTime => _ns;

        public static DcSysTime Now() => FromUtc(DateTime.UtcNow);

        public static DcSysTime FromUtc(DateTime utc)
        {
            var ticks = utc.ToUniversalTime().Ticks - EcatEpoch.Ticks;
            if (ticks < 0)
            {
                throw new Autd3Exception("UTC time is out of the representable DcSysTime range (2000-01-01 0:00:00 UTC ..)");
            }
            return new DcSysTime((ulong)ticks * 100);
        }

        public DateTime ToUtc() => EcatEpoch.AddTicks((long)(_ns / 100));

        public static DcSysTime operator +(DcSysTime lhs, TimeSpan rhs) =>
            new DcSysTime(checked(lhs._ns + (ulong)rhs.Ticks * 100));

        public static DcSysTime operator -(DcSysTime lhs, TimeSpan rhs) =>
            new DcSysTime(checked(lhs._ns - (ulong)rhs.Ticks * 100));

        public bool Equals(DcSysTime other) => _ns == other._ns;

        public override bool Equals(object? obj) => obj is DcSysTime other && Equals(other);

        public override int GetHashCode() => _ns.GetHashCode();

        public int CompareTo(DcSysTime other) => _ns.CompareTo(other._ns);

        public static bool operator ==(DcSysTime left, DcSysTime right) => left.Equals(right);

        public static bool operator !=(DcSysTime left, DcSysTime right) => !left.Equals(right);

        public static bool operator <(DcSysTime left, DcSysTime right) => left._ns < right._ns;

        public static bool operator >(DcSysTime left, DcSysTime right) => left._ns > right._ns;

        public static bool operator <=(DcSysTime left, DcSysTime right) => left._ns <= right._ns;

        public static bool operator >=(DcSysTime left, DcSysTime right) => left._ns >= right._ns;
    }
}
