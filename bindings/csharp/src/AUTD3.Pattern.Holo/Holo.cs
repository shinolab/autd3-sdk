using System;
using System.Collections.Generic;
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

    public readonly struct IntensityConstraint
    {
        internal byte Kind { get; }
        internal byte Min { get; }
        internal byte Max { get; }
        internal float MultiplyValue { get; }

        private IntensityConstraint(byte kind, byte min, byte max, float multiply)
        {
            Kind = kind;
            Min = min;
            Max = max;
            MultiplyValue = multiply;
        }

        public static IntensityConstraint Normalize => new IntensityConstraint(0, 0, 0, 0);
        public static IntensityConstraint Multiply(float value) => new IntensityConstraint(1, 0, 0, value);
        public static IntensityConstraint Uniform(Intensity intensity) => new IntensityConstraint(2, intensity.Value, 0, 0);
        public static IntensityConstraint Clamp(Intensity min, Intensity max) => new IntensityConstraint(3, min.Value, max.Value, 0);

        internal IntensityConstraintNative ToNative() =>
            new IntensityConstraintNative { Kind = Kind, Min = Min, Max = Max, Multiply = MultiplyValue };
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct IntensityConstraintNative
    {
        public byte Kind;
        public byte Min;
        public byte Max;
        public float Multiply;
    }

    public readonly struct NaiveOption
    {
        private readonly IntensityConstraint? _constraint;
        public IntensityConstraint Constraint { get => _constraint ?? IntensityConstraint.Clamp(Intensity.Min, Intensity.Max); init => _constraint = value; }
        private readonly Directivity? _directivity;
        public Directivity Directivity { get => _directivity ?? Directivity.Sphere; init => _directivity = value; }
        private readonly TransducerMask? _mask;
        public TransducerMask Mask { get => _mask ?? TransducerMask.AllEnabled; init => _mask = value; }
        private readonly bool? _parallel;
        public bool Parallel { get => _parallel ?? true; init => _parallel = value; }
    }

    public readonly struct GsOption
    {
        private readonly uint? _repeat;
        public uint Repeat { get => _repeat ?? 100; init => _repeat = value; }
        private readonly IntensityConstraint? _constraint;
        public IntensityConstraint Constraint { get => _constraint ?? IntensityConstraint.Clamp(Intensity.Min, Intensity.Max); init => _constraint = value; }
        private readonly Directivity? _directivity;
        public Directivity Directivity { get => _directivity ?? Directivity.Sphere; init => _directivity = value; }
        private readonly TransducerMask? _mask;
        public TransducerMask Mask { get => _mask ?? TransducerMask.AllEnabled; init => _mask = value; }
        private readonly bool? _parallel;
        public bool Parallel { get => _parallel ?? true; init => _parallel = value; }
    }

    public readonly struct GspatOption
    {
        private readonly uint? _repeat;
        public uint Repeat { get => _repeat ?? 100; init => _repeat = value; }
        private readonly IntensityConstraint? _constraint;
        public IntensityConstraint Constraint { get => _constraint ?? IntensityConstraint.Clamp(Intensity.Min, Intensity.Max); init => _constraint = value; }
        private readonly Directivity? _directivity;
        public Directivity Directivity { get => _directivity ?? Directivity.Sphere; init => _directivity = value; }
        private readonly TransducerMask? _mask;
        public TransducerMask Mask { get => _mask ?? TransducerMask.AllEnabled; init => _mask = value; }
        private readonly bool? _parallel;
        public bool Parallel { get => _parallel ?? true; init => _parallel = value; }
    }

    public readonly struct GreedyOption
    {
        private readonly byte? _phaseQuantizationLevels;
        public byte PhaseQuantizationLevels { get => _phaseQuantizationLevels ?? 16; init => _phaseQuantizationLevels = value; }
        private readonly IntensityConstraint? _constraint;
        public IntensityConstraint Constraint { get => _constraint ?? IntensityConstraint.Uniform(Intensity.Max); init => _constraint = value; }
        private readonly Directivity? _directivity;
        public Directivity Directivity { get => _directivity ?? Directivity.Sphere; init => _directivity = value; }
        private readonly TransducerMask? _mask;
        public TransducerMask Mask { get => _mask ?? TransducerMask.AllEnabled; init => _mask = value; }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct HoloAmplitudeTargetNative
    {
        public float X;
        public float Y;
        public float Z;
        public float AmplitudePa;
    }

    internal static class NativeHolo
    {
        private const string Lib = "autd3_pattern_holo";

        static NativeHolo() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_holo_amplitude_pascal(float value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_holo_amplitude_kilo_pascal(float value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_holo_amplitude_spl(float value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_holo_naive(GeometryHandle geometry, HoloAmplitudeTargetNative[] foci, UIntPtr numFoci, float wavelengthMm, in IntensityConstraintNative constraint, byte directivity, byte[]? mask, [MarshalAs(UnmanagedType.I1)] bool parallel, PhaseBufferHandle phases, IntensityBufferHandle intensities, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_holo_gs(GeometryHandle geometry, HoloAmplitudeTargetNative[] foci, UIntPtr numFoci, float wavelengthMm, UIntPtr repeat, in IntensityConstraintNative constraint, byte directivity, byte[]? mask, [MarshalAs(UnmanagedType.I1)] bool parallel, PhaseBufferHandle phases, IntensityBufferHandle intensities, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_holo_gspat(GeometryHandle geometry, HoloAmplitudeTargetNative[] foci, UIntPtr numFoci, float wavelengthMm, UIntPtr repeat, in IntensityConstraintNative constraint, byte directivity, byte[]? mask, [MarshalAs(UnmanagedType.I1)] bool parallel, PhaseBufferHandle phases, IntensityBufferHandle intensities, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_holo_naive_batch(GeometryHandle geometry, HoloAmplitudeTargetNative[] foci, UIntPtr numFoci, float wavelengthMm, in IntensityConstraintNative constraint, byte directivity, byte[]? mask, [MarshalAs(UnmanagedType.I1)] bool parallel, IntPtr[] phases, IntPtr[] intensities, UIntPtr numProblems, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_holo_gs_batch(GeometryHandle geometry, HoloAmplitudeTargetNative[] foci, UIntPtr numFoci, float wavelengthMm, UIntPtr repeat, in IntensityConstraintNative constraint, byte directivity, byte[]? mask, [MarshalAs(UnmanagedType.I1)] bool parallel, IntPtr[] phases, IntPtr[] intensities, UIntPtr numProblems, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_holo_gspat_batch(GeometryHandle geometry, HoloAmplitudeTargetNative[] foci, UIntPtr numFoci, float wavelengthMm, UIntPtr repeat, in IntensityConstraintNative constraint, byte directivity, byte[]? mask, [MarshalAs(UnmanagedType.I1)] bool parallel, IntPtr[] phases, IntPtr[] intensities, UIntPtr numProblems, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_holo_greedy(GeometryHandle geometry, HoloAmplitudeTargetNative[] foci, UIntPtr numFoci, float wavelengthMm, byte phaseQuantizationLevels, in IntensityConstraintNative constraint, byte directivity, byte[]? mask, PhaseBufferHandle phases, IntensityBufferHandle intensities, byte[] outErr, UIntPtr outErrLen);
    }

    public readonly struct AmplitudeTarget
    {
        public Vector3 Point { get; }
        public Amplitude Amplitude { get; }

        public AmplitudeTarget(Vector3 point, Amplitude amplitude)
        {
            Point = point;
            Amplitude = amplitude;
        }
    }

    public static class Holo
    {

        private static HoloAmplitudeTargetNative[] ToNative(AmplitudeTarget[] foci)
        {
            var native = new HoloAmplitudeTargetNative[foci.Length];
            for (var i = 0; i < foci.Length; i++)
            {
                var p = Coords.Point(foci[i].Point);
                native[i] = new HoloAmplitudeTargetNative
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
            if (mask.Length != numDevices)
            {
                throw new Autd3Exception($"the mask has {mask.Length} device slots but the buffer has {numDevices} devices");
            }
            return BufferArray.Flatten(mask, m => (byte)(m ? 1 : 0), "mask values");
        }

        public static void Naive(Geometry geometry, AmplitudeTarget[] foci, Length wavelength, NaiveOption option, PhaseBuffer phases, IntensityBuffer intensities)
        {
            var c = option.Constraint.ToNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            if (NativeHolo.autd3_holo_naive(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, geometry.NumDevices), option.Parallel, phases.Handle, intensities.Handle, err, (UIntPtr)err.Length) != 0)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
        }

        public static void Gs(Geometry geometry, AmplitudeTarget[] foci, Length wavelength, GsOption option, PhaseBuffer phases, IntensityBuffer intensities)
        {
            var c = option.Constraint.ToNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            if (NativeHolo.autd3_holo_gs(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, (UIntPtr)option.Repeat, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, geometry.NumDevices), option.Parallel, phases.Handle, intensities.Handle, err, (UIntPtr)err.Length) != 0)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
        }

        public static void Gspat(Geometry geometry, AmplitudeTarget[] foci, Length wavelength, GspatOption option, PhaseBuffer phases, IntensityBuffer intensities)
        {
            var c = option.Constraint.ToNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            if (NativeHolo.autd3_holo_gspat(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, (UIntPtr)option.Repeat, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, geometry.NumDevices), option.Parallel, phases.Handle, intensities.Handle, err, (UIntPtr)err.Length) != 0)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
        }

        private static void CheckBatch(IReadOnlyList<PhaseBuffer> phases, IReadOnlyList<IntensityBuffer> intensities)
        {
            if (phases.Count != intensities.Count)
            {
                throw new Autd3Exception(
                    $"a batch needs one intensity buffer per phase buffer, but {phases.Count} phase and {intensities.Count} intensity buffers were given",
                    Autd3ErrorCode.InvalidArgument);
            }
        }

        private static HandleArray Lease<T>(IReadOnlyList<T> buffers, Func<T, SafeHandle> handle)
        {
            var handles = new SafeHandle[buffers.Count];
            for (var i = 0; i < handles.Length; i++)
            {
                handles[i] = handle(buffers[i]);
            }
            return new HandleArray(handles);
        }

        public static void NaiveBatch(Geometry geometry, AmplitudeTarget[] foci, Length wavelength, NaiveOption option, IReadOnlyList<PhaseBuffer> phases, IReadOnlyList<IntensityBuffer> intensities)
        {
            CheckBatch(phases, intensities);
            var c = option.Constraint.ToNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            using var p = Lease(phases, b => b.Handle);
            using var i = Lease(intensities, b => b.Handle);
            var code = NativeHolo.autd3_holo_naive_batch(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, geometry.NumDevices), option.Parallel, p.Pointers, i.Pointers, (UIntPtr)phases.Count, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
        }

        public static void GsBatch(Geometry geometry, AmplitudeTarget[] foci, Length wavelength, GsOption option, IReadOnlyList<PhaseBuffer> phases, IReadOnlyList<IntensityBuffer> intensities)
        {
            CheckBatch(phases, intensities);
            var c = option.Constraint.ToNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            using var p = Lease(phases, b => b.Handle);
            using var i = Lease(intensities, b => b.Handle);
            var code = NativeHolo.autd3_holo_gs_batch(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, (UIntPtr)option.Repeat, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, geometry.NumDevices), option.Parallel, p.Pointers, i.Pointers, (UIntPtr)phases.Count, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
        }

        public static void GspatBatch(Geometry geometry, AmplitudeTarget[] foci, Length wavelength, GspatOption option, IReadOnlyList<PhaseBuffer> phases, IReadOnlyList<IntensityBuffer> intensities)
        {
            CheckBatch(phases, intensities);
            var c = option.Constraint.ToNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            using var p = Lease(phases, b => b.Handle);
            using var i = Lease(intensities, b => b.Handle);
            var code = NativeHolo.autd3_holo_gspat_batch(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, (UIntPtr)option.Repeat, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, geometry.NumDevices), option.Parallel, p.Pointers, i.Pointers, (UIntPtr)phases.Count, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
        }

        public static void Greedy(Geometry geometry, AmplitudeTarget[] foci, Length wavelength, GreedyOption option, PhaseBuffer phases, IntensityBuffer intensities)
        {
            var c = option.Constraint.ToNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            if (NativeHolo.autd3_holo_greedy(geometry.Handle, ToNative(foci), (UIntPtr)foci.Length, wavelength.Mm, option.PhaseQuantizationLevels, in c, (byte)option.Directivity, FlattenMask(option.Mask.Mask, geometry.NumDevices), phases.Handle, intensities.Handle, err, (UIntPtr)err.Length) != 0)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
        }
    }
}
