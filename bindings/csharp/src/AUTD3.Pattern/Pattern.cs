using System;
using System.Numerics;
using System.Runtime.InteropServices;

namespace AUTD3
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct PatternOptionNative
    {
        public byte Intensity;
        public byte PhaseOffset;
    }

    public readonly struct FocusOption
    {
        public Intensity Intensity { get; }
        public Phase PhaseOffset { get; }

        public FocusOption(Intensity? intensity = null, Phase? phaseOffset = null)
        {
            Intensity = intensity ?? Intensity.Max;
            PhaseOffset = phaseOffset ?? Phase.Zero;
        }

        internal PatternOptionNative ToNative() =>
            new PatternOptionNative { Intensity = Intensity.Value, PhaseOffset = PhaseOffset.Value };
    }

    public readonly struct PlaneOption
    {
        public Intensity Intensity { get; }
        public Phase PhaseOffset { get; }

        public PlaneOption(Intensity? intensity = null, Phase? phaseOffset = null)
        {
            Intensity = intensity ?? Intensity.Max;
            PhaseOffset = phaseOffset ?? Phase.Zero;
        }

        internal PatternOptionNative ToNative() =>
            new PatternOptionNative { Intensity = Intensity.Value, PhaseOffset = PhaseOffset.Value };
    }

    public readonly struct BesselOption
    {
        public Intensity Intensity { get; }
        public Phase PhaseOffset { get; }

        public BesselOption(Intensity? intensity = null, Phase? phaseOffset = null)
        {
            Intensity = intensity ?? Intensity.Max;
            PhaseOffset = phaseOffset ?? Phase.Zero;
        }

        internal PatternOptionNative ToNative() =>
            new PatternOptionNative { Intensity = Intensity.Value, PhaseOffset = PhaseOffset.Value };
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct EmissionNative
    {
        public byte Phase;
        public byte Intensity;
    }

    internal static class NativePattern
    {
        private const string Lib = "autd3_pattern";

        [DllImport(Lib)]
        internal static extern float autd3_pattern_wavelength(float soundSpeedMmPerS);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_core_geometry_pattern_buffer(IntPtr geometry);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_pattern_buffer_from_array(EmissionNative[] emissions, UIntPtr numDevices);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_pattern_buffer_num_devices(IntPtr buffer);

        [DllImport(Lib)]
        internal static extern void autd3_pattern_buffer_free(IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_pattern_focus(IntPtr geometry, float[] target, float wavelengthMm, in PatternOptionNative option, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_pattern_plane(IntPtr geometry, float[] dir, float wavelengthMm, in PatternOptionNative option, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_pattern_bessel(IntPtr geometry, float[] apex, float[] dir, float thetaRad, float wavelengthMm, in PatternOptionNative option, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_pattern_uniform(byte phase, byte intensity, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern void autd3_pattern_null(IntPtr buffer);


        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_pattern(byte bank, IntPtr patternBuffer);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_write_pattern_buffer(byte bank, ushort index, IntPtr patternBuffer);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_write_pattern_compressed(byte bank, uint index, byte format, IntPtr[] patterns, UIntPtr numPatterns);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_config_pattern(byte bank, IntPtr samplingConfig, uint size, byte dataTypeKind, byte numFoci, ushort soundSpeed, ushort rep);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_change_pattern_bank(byte bank, byte transitionMode, ulong transitionValue);
    }

    public sealed class PatternBuffer : IDisposable
    {
        internal const int NumTransducers = 249;

        internal IntPtr Handle { get; private set; }

        internal PatternBuffer(IntPtr handle)
        {
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create pattern buffer");
            }
            Handle = handle;
        }

        public static PatternBuffer FromArray(Emission[][] emissions)
        {
            var numDevices = emissions.Length;
            var flat = new EmissionNative[numDevices * NumTransducers];
            for (var d = 0; d < numDevices; d++)
            {
                if (emissions[d].Length != NumTransducers)
                {
                    throw new Autd3Exception($"each device requires {NumTransducers} emissions");
                }
                for (var t = 0; t < NumTransducers; t++)
                {
                    flat[d * NumTransducers + t] = new EmissionNative
                    {
                        Phase = emissions[d][t].Phase.Value,
                        Intensity = emissions[d][t].Intensity.Value,
                    };
                }
            }
            return new PatternBuffer(NativePattern.autd3_pattern_buffer_from_array(flat, (UIntPtr)numDevices));
        }

        public int NumDevices => (int)NativePattern.autd3_pattern_buffer_num_devices(Handle);

        public void Dispose()
        {
            if (Handle != IntPtr.Zero)
            {
                NativePattern.autd3_pattern_buffer_free(Handle);
                Handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~PatternBuffer()
        {
            if (Handle != IntPtr.Zero)
            {
                NativePattern.autd3_pattern_buffer_free(Handle);
            }
        }
    }

    public static class GeometryPatternBufferExtensions
    {
        public static PatternBuffer PatternBuffer(this Geometry geometry) =>
            new PatternBuffer(NativePattern.autd3_core_geometry_pattern_buffer(geometry.Handle));
    }




    public sealed class Pattern : ICommand
    {
        private readonly PatternBank _bank;
        private readonly PatternBuffer _buffer;

        public Pattern(PatternBuffer buffer, PatternBank bank = PatternBank.B0)
        {
            _bank = bank;
            _buffer = buffer;
        }

        IntPtr ICommand.CreateOp() => NativePattern.autd3_op_pattern((byte)_bank, _buffer.Handle);


        public static Length Wavelength(Velocity soundSpeed) =>
            new Length(NativePattern.autd3_pattern_wavelength(soundSpeed.MmPerSec));


        public static void Focus(Geometry geometry, Vector3 target, Length wavelength, FocusOption option, PatternBuffer buffer)
        {
            var t = new[] { target.X, target.Y, target.Z };
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_focus(geometry.Handle, t, wavelength.Mm, in o, buffer.Handle) != 0)
            {
                throw new Autd3Exception("focus failed (buffer device count must match geometry)");
            }
        }

        public static void Focus(Geometry geometry, Vector3 target, Length wavelength, Intensity intensity, PatternBuffer buffer) =>
            Focus(geometry, target, wavelength, new FocusOption(intensity), buffer);

        public static void Plane(Geometry geometry, Vector3 dir, Length wavelength, PlaneOption option, PatternBuffer buffer)
        {
            var d = new[] { dir.X, dir.Y, dir.Z };
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_plane(geometry.Handle, d, wavelength.Mm, in o, buffer.Handle) != 0)
            {
                throw new Autd3Exception("plane failed (buffer device count must match geometry)");
            }
        }

        public static void Bessel(Geometry geometry, Vector3 apex, Vector3 dir, Angle theta, Length wavelength, BesselOption option, PatternBuffer buffer)
        {
            var a = new[] { apex.X, apex.Y, apex.Z };
            var d = new[] { dir.X, dir.Y, dir.Z };
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_bessel(geometry.Handle, a, d, theta.Radian, wavelength.Mm, in o, buffer.Handle) != 0)
            {
                throw new Autd3Exception("bessel failed (buffer device count must match geometry)");
            }
        }

        public static void Uniform(Emission emission, PatternBuffer buffer)
        {
            if (NativePattern.autd3_pattern_uniform(emission.Phase.Value, emission.Intensity.Value, buffer.Handle) != 0)
            {
                throw new Autd3Exception("uniform failed");
            }
        }


        public static void Null(PatternBuffer buffer) => NativePattern.autd3_pattern_null(buffer.Handle);
    }
}
