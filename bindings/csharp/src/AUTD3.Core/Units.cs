using System;

namespace AUTD3
{
    public readonly struct Length : IEquatable<Length>
    {
        public float Mm { get; }

        internal Length(float mm)
        {
            Mm = mm;
        }

        public float M => Mm / 1000f;

        public static Length FromMm(float mm) => new Length(mm);

        public static Length FromM(float m) => new Length(m * 1000f);

        public static Length operator +(Length lhs, Length rhs) => new Length(lhs.Mm + rhs.Mm);

        public static Length operator -(Length lhs, Length rhs) => new Length(lhs.Mm - rhs.Mm);

        public static Length operator -(Length value) => new Length(-value.Mm);

        public static Length operator *(Length lhs, float rhs) => new Length(lhs.Mm * rhs);

        public static Length operator *(float lhs, Length rhs) => new Length(lhs * rhs.Mm);

        public static Length operator /(Length lhs, float rhs) => new Length(lhs.Mm / rhs);

        public bool Equals(Length other) => Mm.Equals(other.Mm);

        public override bool Equals(object? obj) => obj is Length other && Equals(other);

        public override int GetHashCode() => Mm.GetHashCode();

        public static bool operator ==(Length left, Length right) => left.Mm == right.Mm;

        public static bool operator !=(Length left, Length right) => left.Mm != right.Mm;

        public static bool operator <(Length left, Length right) => left.Mm < right.Mm;

        public static bool operator >(Length left, Length right) => left.Mm > right.Mm;

        public static bool operator <=(Length left, Length right) => left.Mm <= right.Mm;

        public static bool operator >=(Length left, Length right) => left.Mm >= right.Mm;
    }

    public readonly struct Angle : IEquatable<Angle>
    {
        private const float DegreesPerRadian = 57.29577951308232f;

        public float Rad { get; }

        internal Angle(float radian)
        {
            Rad = radian;
        }

        public float Deg => Rad * DegreesPerRadian;

        public static Angle Zero => new Angle(0f);

        public static Angle FromRad(float radian) => new Angle(radian);

        public static Angle FromDeg(float degree) => new Angle(degree * (MathF.PI / 180f));

        public static Angle operator +(Angle lhs, Angle rhs) => new Angle(lhs.Rad + rhs.Rad);

        public static Angle operator -(Angle lhs, Angle rhs) => new Angle(lhs.Rad - rhs.Rad);

        public static Angle operator -(Angle value) => new Angle(-value.Rad);

        public static Angle operator *(Angle lhs, float rhs) => new Angle(lhs.Rad * rhs);

        public static Angle operator *(float lhs, Angle rhs) => new Angle(rhs.Rad * lhs);

        public static Angle operator /(Angle lhs, float rhs) => new Angle(lhs.Rad / rhs);

        public bool Equals(Angle other) => Rad.Equals(other.Rad);

        public override bool Equals(object? obj) => obj is Angle other && Equals(other);

        public override int GetHashCode() => Rad.GetHashCode();

        public static bool operator ==(Angle left, Angle right) => left.Rad == right.Rad;

        public static bool operator !=(Angle left, Angle right) => left.Rad != right.Rad;
    }

    public readonly struct Velocity : IEquatable<Velocity>
    {
        public float MmS { get; }

        internal Velocity(float mmPerS)
        {
            MmS = mmPerS;
        }

        public float MS => MmS / 1000f;

        public static Velocity FromMmS(float mmPerS) => new Velocity(mmPerS);

        public static Velocity FromMS(float mPerS) => new Velocity(mPerS * 1000f);

        public bool Equals(Velocity other) => MmS.Equals(other.MmS);

        public override bool Equals(object? obj) => obj is Velocity other && Equals(other);

        public override int GetHashCode() => MmS.GetHashCode();

        public static bool operator ==(Velocity left, Velocity right) => left.MmS == right.MmS;

        public static bool operator !=(Velocity left, Velocity right) => left.MmS != right.MmS;

        public static bool operator <(Velocity left, Velocity right) => left.MmS < right.MmS;

        public static bool operator >(Velocity left, Velocity right) => left.MmS > right.MmS;

        public static bool operator <=(Velocity left, Velocity right) => left.MmS <= right.MmS;

        public static bool operator >=(Velocity left, Velocity right) => left.MmS >= right.MmS;
    }

    public readonly struct Freq : IEquatable<Freq>
    {
        public float Hz { get; }

        internal Freq(float hz)
        {
            Hz = hz;
        }

        public static Freq FromHz(float hz) => new Freq(hz);

        public static Freq operator +(Freq lhs, Freq rhs) => new Freq(lhs.Hz + rhs.Hz);

        public static Freq operator -(Freq lhs, Freq rhs) => new Freq(lhs.Hz - rhs.Hz);

        public static Freq operator *(Freq lhs, float rhs) => new Freq(lhs.Hz * rhs);

        public static Freq operator *(float lhs, Freq rhs) => new Freq(lhs * rhs.Hz);

        public static Freq operator /(Freq lhs, float rhs) => new Freq(lhs.Hz / rhs);

        public bool Equals(Freq other) => Hz.Equals(other.Hz);

        public override bool Equals(object? obj) => obj is Freq other && Equals(other);

        public override int GetHashCode() => Hz.GetHashCode();

        public static bool operator ==(Freq left, Freq right) => left.Hz == right.Hz;

        public static bool operator !=(Freq left, Freq right) => left.Hz != right.Hz;

        public static bool operator <(Freq left, Freq right) => left.Hz < right.Hz;

        public static bool operator >(Freq left, Freq right) => left.Hz > right.Hz;

        public static bool operator <=(Freq left, Freq right) => left.Hz <= right.Hz;

        public static bool operator >=(Freq left, Freq right) => left.Hz >= right.Hz;
    }

    public readonly struct Nearest<T>
    {
        public T Value { get; }

        public Nearest(T value)
        {
            Value = value;
        }
    }

    public readonly struct LengthUnit
    {
        internal float MmPerUnit { get; }

        internal LengthUnit(float mmPerUnit)
        {
            MmPerUnit = mmPerUnit;
        }

        public static Length operator *(float value, LengthUnit unit) => new Length(value * unit.MmPerUnit);

        public static Length operator *(int value, LengthUnit unit) => new Length(value * unit.MmPerUnit);
    }

    public readonly struct AngleUnit
    {
        internal float RadPerUnit { get; }

        internal AngleUnit(float radPerUnit)
        {
            RadPerUnit = radPerUnit;
        }

        public static Angle operator *(float value, AngleUnit unit) => new Angle(value * unit.RadPerUnit);

        public static Angle operator *(int value, AngleUnit unit) => new Angle(value * unit.RadPerUnit);
    }

    public readonly struct FreqUnit
    {
        internal float HzPerUnit { get; }

        internal FreqUnit(float hzPerUnit)
        {
            HzPerUnit = hzPerUnit;
        }

        public static Freq operator *(float value, FreqUnit unit) => new Freq(value * unit.HzPerUnit);
    }

    public readonly struct SecUnit
    {
        public static Velocity operator /(Length length, SecUnit unit)
        {
            _ = unit;
            return new Velocity(length.Mm);
        }
    }

    public static class Units
    {
        public static readonly LengthUnit m = new LengthUnit(1000f);
        public static readonly LengthUnit mm = new LengthUnit(1f);
        public static readonly AngleUnit rad = new AngleUnit(1f);
        public static readonly AngleUnit deg = new AngleUnit((float)(Math.PI / 180.0));
        public static readonly FreqUnit Hz = new FreqUnit(1f);
        public static readonly FreqUnit kHz = new FreqUnit(1000f);
        public static readonly SecUnit s = default;

        public static Vector3 Point(Length x, Length y, Length z) => Coords.FromLengths(x, y, z);

        public static Vector3 Offset(Length x, Length y, Length z) => Coords.FromLengths(x, y, z);

        public static Nearest<Freq> Nearest(Freq freq) => new Nearest<Freq>(freq);
    }
}
