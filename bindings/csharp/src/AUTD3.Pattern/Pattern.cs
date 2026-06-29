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

    internal static class NativePattern
    {
        private const string Lib = "autd3_pattern";

        [DllImport(Lib)]
        internal static extern float autd3_pattern_wavelength(float soundSpeedMmPerS);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_core_geometry_pattern_buffer(IntPtr geometry);

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
        internal static extern IntPtr autd3_op_pattern(IntPtr patternBuffer);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_write_pattern_buffer(byte bank, ushort index, IntPtr patternBuffer);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_config_pattern(byte bank, IntPtr samplingConfig, uint size, byte dataTypeKind, byte numFoci, ushort soundSpeed, ushort rep);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_change_pattern_bank(byte bank, byte transitionMode, ulong transitionValue);
    }

    public sealed class PatternBuffer : IDisposable
    {
        internal IntPtr Handle { get; private set; }

        internal PatternBuffer(IntPtr handle)
        {
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create pattern buffer");
            }
            Handle = handle;
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
        private readonly PatternBuffer _buffer;

        public Pattern(PatternBuffer buffer)
        {
            _buffer = buffer;
        }

        IntPtr ICommand.CreateOp() => NativePattern.autd3_op_pattern(_buffer.Handle);


        public static float Wavelength(float soundSpeedMmPerS) =>
            NativePattern.autd3_pattern_wavelength(soundSpeedMmPerS);


        public static void Focus(Geometry geometry, Vector3 target, float wavelengthMm, FocusOption option, PatternBuffer buffer)
        {
            var t = new[] { target.X, target.Y, target.Z };
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_focus(geometry.Handle, t, wavelengthMm, in o, buffer.Handle) != 0)
            {
                throw new Autd3Exception("focus failed (buffer device count must match geometry)");
            }
        }

        public static void Focus(Geometry geometry, Vector3 target, float wavelengthMm, Intensity intensity, PatternBuffer buffer) =>
            Focus(geometry, target, wavelengthMm, new FocusOption(intensity), buffer);

        public static void Plane(Geometry geometry, Vector3 dir, float wavelengthMm, PlaneOption option, PatternBuffer buffer)
        {
            var d = new[] { dir.X, dir.Y, dir.Z };
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_plane(geometry.Handle, d, wavelengthMm, in o, buffer.Handle) != 0)
            {
                throw new Autd3Exception("plane failed (buffer device count must match geometry)");
            }
        }

        public static void Bessel(Geometry geometry, Vector3 apex, Vector3 dir, float thetaRad, float wavelengthMm, BesselOption option, PatternBuffer buffer)
        {
            var a = new[] { apex.X, apex.Y, apex.Z };
            var d = new[] { dir.X, dir.Y, dir.Z };
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_bessel(geometry.Handle, a, d, thetaRad, wavelengthMm, in o, buffer.Handle) != 0)
            {
                throw new Autd3Exception("bessel failed (buffer device count must match geometry)");
            }
        }

        public static void Uniform(Phase phase, Intensity intensity, PatternBuffer buffer)
        {
            if (NativePattern.autd3_pattern_uniform(phase.Value, intensity.Value, buffer.Handle) != 0)
            {
                throw new Autd3Exception("uniform failed");
            }
        }


        public static void Null(PatternBuffer buffer) => NativePattern.autd3_pattern_null(buffer.Handle);
    }
}
