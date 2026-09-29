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

        public Autd3(Vector3 origin) : this(origin, Coords.IdentityRotation)
        {
        }

        public Autd3(Vector3 origin, Quaternion rotation)
        {
            Origin = origin;
            Rotation = rotation;
        }

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

        public float Rad() => NativeCore.autd3_core_phase_radian(Value);

        public static explicit operator Phase(Angle angle)
        {
            var p = (int)MathF.Round(angle.Rad / (2f * MathF.PI) * 256f);
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
        private static readonly DateTime Epoch = new DateTime(2000, 1, 1, 0, 0, 0, DateTimeKind.Utc);

        private readonly ulong _ns;

        private SysTime(ulong ns)
        {
            _ns = ns;
        }

        public static SysTime Zero => new SysTime(0);

        public static SysTime FromNanos(ulong ns) => new SysTime(ns);

        public ulong Nanos => _ns;

        public static SysTime Now() => FromUtc(DateTime.UtcNow);

        public static SysTime FromUtc(DateTime utc)
        {
            var ticks = utc.ToUniversalTime().Ticks - Epoch.Ticks;
            if (ticks < 0)
            {
                throw new Autd3Exception("UTC time is out of the representable SysTime range (2000-01-01 0:00:00 UTC ..)");
            }
            return new SysTime((ulong)ticks * 100);
        }

        public DateTime ToUtc() => Epoch.AddTicks((long)(_ns / 100));

        public static SysTime operator +(SysTime lhs, TimeSpan rhs) =>
            new SysTime(checked(lhs._ns + (ulong)rhs.Ticks * 100));

        public static SysTime operator -(SysTime lhs, TimeSpan rhs) =>
            new SysTime(checked(lhs._ns - (ulong)rhs.Ticks * 100));

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
