using System;
using System.Numerics;
using System.Runtime.InteropServices;

namespace AUTD3.Holo
{
    public enum Directivity : byte
    {
        Sphere = 0,
        T4010A1 = 1,
    }

    public readonly struct Amplitude
    {
        internal const float AbsoluteThresholdOfHearing = 20e-6f;

        public float Pascal { get; }

        private Amplitude(float pascal)
        {
            Pascal = pascal;
        }

        public static Amplitude FromPascal(float value) =>
            new Amplitude(NativeHolo.autd3_holo_amplitude_pascal(value));

        public static Amplitude FromKiloPascal(float value) =>
            new Amplitude(NativeHolo.autd3_holo_amplitude_kilo_pascal(value));

        public static Amplitude FromSpl(float value) =>
            new Amplitude(NativeHolo.autd3_holo_amplitude_spl(value));

        public float Spl => 20f * MathF.Log10(Pascal / AbsoluteThresholdOfHearing);
    }

    public readonly struct PressureUnit
    {
        internal float PaPerUnit { get; }

        internal PressureUnit(float paPerUnit)
        {
            PaPerUnit = paPerUnit;
        }

        public static Amplitude operator *(float value, PressureUnit unit) => Amplitude.FromPascal(value * unit.PaPerUnit);

        public static Amplitude operator *(int value, PressureUnit unit) => Amplitude.FromPascal(value * unit.PaPerUnit);
    }

    public readonly struct SplUnit
    {
        public static Amplitude operator *(float value, SplUnit unit)
        {
            _ = unit;
            return Amplitude.FromSpl(value);
        }

        public static Amplitude operator *(int value, SplUnit unit)
        {
            _ = unit;
            return Amplitude.FromSpl(value);
        }
    }

    public static class HoloUnits
    {
        public static readonly PressureUnit Pa = new PressureUnit(1f);
        public static readonly PressureUnit kPa = new PressureUnit(1000f);
        public static readonly SplUnit dB = default;
    }

    public readonly struct EmissionConstraint
    {
        internal byte Kind { get; }
        internal byte Min { get; }
        internal byte Max { get; }
        internal float MultiplyValue { get; }

        private EmissionConstraint(byte kind, byte min, byte max, float multiply)
        {
            Kind = kind;
            Min = min;
            Max = max;
            MultiplyValue = multiply;
        }

        public static EmissionConstraint Normalize => new EmissionConstraint(0, 0, 0, 0);
        public static EmissionConstraint Multiply(float value) => new EmissionConstraint(1, 0, 0, value);
        public static EmissionConstraint Uniform(Intensity intensity) => new EmissionConstraint(2, intensity.Value, 0, 0);
        public static EmissionConstraint Clamp(Intensity min, Intensity max) => new EmissionConstraint(3, min.Value, max.Value, 0);

        internal EmissionConstraintNative ToNative() =>
            new EmissionConstraintNative { Kind = Kind, Min = Min, Max = Max, Multiply = MultiplyValue };
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct EmissionConstraintNative
    {
        public byte Kind;
        public byte Min;
        public byte Max;
        public float Multiply;
    }

    public readonly struct TransducerMask
    {
        internal bool[][]? Mask { get; }

        private TransducerMask(bool[][]? mask)
        {
            Mask = mask;
        }

        public static TransducerMask AllEnabled => new TransducerMask(null);

        public static TransducerMask Masked(bool[][] mask) => new TransducerMask(mask);
    }

    public readonly struct NaiveOption
    {
        public EmissionConstraint Constraint { get; }
        public Directivity Directivity { get; }
        public TransducerMask Mask { get; }
        public bool Parallel { get; }

        public NaiveOption() : this(constraint: null)
        {
        }

        public NaiveOption(EmissionConstraint? constraint = null, Directivity directivity = Directivity.Sphere, TransducerMask mask = default, bool parallel = true)
        {
            Constraint = constraint ?? EmissionConstraint.Clamp(Intensity.Min, Intensity.Max);
            Directivity = directivity;
            Mask = mask;
            Parallel = parallel;
        }
    }

    public readonly struct GsOption
    {
        public uint Repeat { get; }
        public EmissionConstraint Constraint { get; }
        public Directivity Directivity { get; }
        public TransducerMask Mask { get; }
        public bool Parallel { get; }

        public GsOption() : this(repeat: 100)
        {
        }

        public GsOption(uint repeat = 100, EmissionConstraint? constraint = null, Directivity directivity = Directivity.Sphere, TransducerMask mask = default, bool parallel = true)
        {
            Repeat = repeat;
            Constraint = constraint ?? EmissionConstraint.Clamp(Intensity.Min, Intensity.Max);
            Directivity = directivity;
            Mask = mask;
            Parallel = parallel;
        }
    }

    public readonly struct GspatOption
    {
        public uint Repeat { get; }
        public EmissionConstraint Constraint { get; }
        public Directivity Directivity { get; }
        public TransducerMask Mask { get; }
        public bool Parallel { get; }

        public GspatOption() : this(repeat: 100)
        {
        }

        public GspatOption(uint repeat = 100, EmissionConstraint? constraint = null, Directivity directivity = Directivity.Sphere, TransducerMask mask = default, bool parallel = true)
        {
            Repeat = repeat;
            Constraint = constraint ?? EmissionConstraint.Clamp(Intensity.Min, Intensity.Max);
            Directivity = directivity;
            Mask = mask;
            Parallel = parallel;
        }
    }

    public readonly struct GreedyOption
    {
        public byte PhaseQuantizationLevels { get; }
        public EmissionConstraint Constraint { get; }
        public Directivity Directivity { get; }
        public TransducerMask Mask { get; }

        public GreedyOption() : this(phaseQuantizationLevels: 16)
        {
        }

        public GreedyOption(byte phaseQuantizationLevels = 16, EmissionConstraint? constraint = null, Directivity directivity = Directivity.Sphere, TransducerMask mask = default)
        {
            PhaseQuantizationLevels = phaseQuantizationLevels;
            Constraint = constraint ?? EmissionConstraint.Uniform(Intensity.Max);
            Directivity = directivity;
            Mask = mask;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct HoloControlPointNative
    {
        public float X;
        public float Y;
        public float Z;
        public float AmplitudePa;
    }

    internal static class NativeHolo
    {
        private const string Lib = "autd3_pattern_holo";

        [DllImport(Lib)]
        internal static extern float autd3_holo_amplitude_pascal(float value);

        [DllImport(Lib)]
        internal static extern float autd3_holo_amplitude_kilo_pascal(float value);

        [DllImport(Lib)]
        internal static extern float autd3_holo_amplitude_spl(float value);

        [DllImport(Lib)]
        internal static extern int autd3_holo_naive(IntPtr geometry, HoloControlPointNative[] foci, UIntPtr numFoci, float wavelengthMm, in EmissionConstraintNative constraint, byte directivity, byte[]? mask, [MarshalAs(UnmanagedType.I1)] bool parallel, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_holo_gs(IntPtr geometry, HoloControlPointNative[] foci, UIntPtr numFoci, float wavelengthMm, UIntPtr repeat, in EmissionConstraintNative constraint, byte directivity, byte[]? mask, [MarshalAs(UnmanagedType.I1)] bool parallel, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_holo_gspat(IntPtr geometry, HoloControlPointNative[] foci, UIntPtr numFoci, float wavelengthMm, UIntPtr repeat, in EmissionConstraintNative constraint, byte directivity, byte[]? mask, [MarshalAs(UnmanagedType.I1)] bool parallel, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_holo_greedy(IntPtr geometry, HoloControlPointNative[] foci, UIntPtr numFoci, float wavelengthMm, byte phaseQuantizationLevels, in EmissionConstraintNative constraint, byte directivity, byte[]? mask, IntPtr buffer);
    }

    public readonly struct ControlPoint
    {
        public Vector3 Point { get; }
        public Amplitude Amplitude { get; }

        public ControlPoint(Vector3 point, Amplitude amplitude)
        {
            Point = point;
            Amplitude = amplitude;
        }
    }

    public static class Holo
    {

        private static HoloControlPointNative[] ToNative(ControlPoint[] foci)
        {
            var native = new HoloControlPointNative[foci.Length];
            for (var i = 0; i < foci.Length; i++)
            {
                var p = Coords.Point(foci[i].Point);
                native[i] = new HoloControlPointNative
                {
                    X = p.X,
                    Y = p.Y,
                    Z = p.Z,
                    AmplitudePa = foci[i].Amplitude.Pascal,
                };
            }
            return native;
        }

        private static byte[]? FlattenMask(bool[][]? mask, int numDevices)
        {
            if (mask == null)
            {
                return null;
            }
            var flat = new byte[numDevices * Autd3.NumTransducers];
            for (var d = 0; d < numDevices; d++)
            {
                if (mask[d].Length != Autd3.NumTransducers)
                {
                    throw new Autd3Exception($"each device mask requires {Autd3.NumTransducers} values");
                }
                for (var t = 0; t < Autd3.NumTransducers; t++)
                {
                    flat[d * Autd3.NumTransducers + t] = (byte)(mask[d][t] ? 1 : 0);
                }
            }
            return flat;
        }

        public static void Naive(Geometry geometry, ControlPoint[] foci, Length wavelength, NaiveOption option, PatternBuffer dst)
        {
            var c = option.Constraint.ToNative();
            if (NativeHolo.autd3_holo_naive(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, dst.NumDevices), option.Parallel, dst.Handle) != 0)
            {
                throw new Autd3Exception("naive failed");
            }
        }

        public static void Gs(Geometry geometry, ControlPoint[] foci, Length wavelength, GsOption option, PatternBuffer dst)
        {
            var c = option.Constraint.ToNative();
            if (NativeHolo.autd3_holo_gs(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, (UIntPtr)option.Repeat, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, dst.NumDevices), option.Parallel, dst.Handle) != 0)
            {
                throw new Autd3Exception("gs failed");
            }
        }

        public static void Gspat(Geometry geometry, ControlPoint[] foci, Length wavelength, GspatOption option, PatternBuffer dst)
        {
            var c = option.Constraint.ToNative();
            if (NativeHolo.autd3_holo_gspat(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, (UIntPtr)option.Repeat, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, dst.NumDevices), option.Parallel, dst.Handle) != 0)
            {
                throw new Autd3Exception("gspat failed");
            }
        }

        public static void Greedy(Geometry geometry, ControlPoint[] foci, Length wavelength, GreedyOption option, PatternBuffer dst)
        {
            var c = option.Constraint.ToNative();
            if (NativeHolo.autd3_holo_greedy(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, option.PhaseQuantizationLevels, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, dst.NumDevices), dst.Handle) != 0)
            {
                throw new Autd3Exception("greedy failed");
            }
        }
    }
}
