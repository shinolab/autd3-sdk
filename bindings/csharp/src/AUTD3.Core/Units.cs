using System;

namespace AUTD3
{
    public readonly struct Length
    {
        public float Mm { get; }

        internal Length(float mm)
        {
            Mm = mm;
        }

        public float M => Mm / 1000f;

        public static Length Millimeters(float mm) => new Length(mm);
    }

    public readonly struct Angle
    {
        public float Radian { get; }

        internal Angle(float radian)
        {
            Radian = radian;
        }

        public float Degree => Radian * (180f / MathF.PI);

        public static Angle Zero => new Angle(0f);

        public static Angle Pi => new Angle(MathF.PI);

        public static Angle FromRadian(float radian) => new Angle(radian);

        public static Angle FromDegree(float degree) => new Angle(degree * (MathF.PI / 180f));
    }

    public readonly struct Velocity
    {
        public float MmPerS { get; }

        internal Velocity(float mmPerS)
        {
            MmPerS = mmPerS;
        }

        public float MPerS => MmPerS / 1000f;

        public static Velocity FromMmS(float mmPerS) => new Velocity(mmPerS);

        public static Velocity FromMS(float mPerS) => new Velocity(mPerS * 1000f);
    }

    public readonly struct Freq
    {
        public enum FreqMode : byte
        {
            FloatExact = 0,
            IntExact = 1,
        }

        public FreqMode Mode { get; }
        internal float HzValue { get; }
        public uint HzIntValue { get; }

        internal Freq(FreqMode mode, float hz, uint hzInt)
        {
            Mode = mode;
            HzValue = hz;
            HzIntValue = hzInt;
        }

        public float Hz => HzValue;

        internal byte ModeCode => (byte)Mode;

        public static Freq operator +(Freq lhs, Freq rhs) =>
            lhs.Mode == FreqMode.IntExact && rhs.Mode == FreqMode.IntExact
                ? new Freq(FreqMode.IntExact, lhs.HzValue + rhs.HzValue, lhs.HzIntValue + rhs.HzIntValue)
                : new Freq(FreqMode.FloatExact, lhs.HzValue + rhs.HzValue, 0);

        public static Freq operator -(Freq lhs, Freq rhs) =>
            lhs.Mode == FreqMode.IntExact && rhs.Mode == FreqMode.IntExact
                ? new Freq(FreqMode.IntExact, lhs.HzValue - rhs.HzValue, lhs.HzIntValue - rhs.HzIntValue)
                : new Freq(FreqMode.FloatExact, lhs.HzValue - rhs.HzValue, 0);

        public static Freq operator *(Freq lhs, uint rhs) =>
            lhs.Mode == FreqMode.IntExact
                ? new Freq(FreqMode.IntExact, lhs.HzValue * rhs, lhs.HzIntValue * rhs)
                : new Freq(FreqMode.FloatExact, lhs.HzValue * rhs, 0);

        public static Freq operator *(Freq lhs, float rhs) =>
            new Freq(FreqMode.FloatExact, lhs.HzValue * rhs, 0);

        public static Freq operator /(Freq lhs, uint rhs) =>
            lhs.Mode == FreqMode.IntExact
                ? new Freq(FreqMode.IntExact, lhs.HzIntValue / rhs, lhs.HzIntValue / rhs)
                : new Freq(FreqMode.FloatExact, lhs.HzValue / rhs, 0);

        public static Freq operator /(Freq lhs, float rhs) =>
            new Freq(FreqMode.FloatExact, lhs.HzValue / rhs, 0);
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
        internal uint HzPerUnitInt { get; }

        internal FreqUnit(float hzPerUnit, uint hzPerUnitInt)
        {
            HzPerUnit = hzPerUnit;
            HzPerUnitInt = hzPerUnitInt;
        }

        public static Freq operator *(int value, FreqUnit unit) =>
            new Freq(Freq.FreqMode.IntExact, value * unit.HzPerUnit, (uint)value * unit.HzPerUnitInt);

        public static Freq operator *(float value, FreqUnit unit) =>
            new Freq(Freq.FreqMode.FloatExact, value * unit.HzPerUnit, 0);
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
        public static readonly FreqUnit Hz = new FreqUnit(1f, 1u);
        public static readonly FreqUnit kHz = new FreqUnit(1000f, 1000u);
        public static readonly SecUnit s = default;

        public static Nearest<Freq> Nearest(Freq freq) => new Nearest<Freq>(freq);

        public static Nearest<TimeSpan> Nearest(TimeSpan period) => new Nearest<TimeSpan>(period);
    }
}
