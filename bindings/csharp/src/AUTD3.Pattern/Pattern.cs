using System;
using System.Numerics;
using System.Runtime.InteropServices;

namespace AUTD3
{
    internal static class NativePattern
    {
        private const string Lib = "autd3_pattern";

        [DllImport(Lib)]
        internal static extern float autd3_pattern_wavelength(float soundSpeedMmPerS);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_pattern_buffer_new(UIntPtr numDevices);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_pattern_buffer_num_devices(IntPtr buffer);

        [DllImport(Lib)]
        internal static extern void autd3_pattern_buffer_free(IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_pattern_focus(IntPtr geometry, float[] target, float wavelengthMm, byte intensity, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern void autd3_pattern_null(IntPtr buffer);


        [DllImport("autd3")]
        internal static extern IntPtr autd3_op_pattern(IntPtr patternBuffer);

        [DllImport("autd3")]
        internal static extern IntPtr autd3_op_write_pattern_buffer(byte bank, ushort index, IntPtr patternBuffer);

        [DllImport("autd3")]
        internal static extern IntPtr autd3_op_config_pattern(byte bank, ushort divider, uint size, byte dataTypeKind, byte numFoci, ushort soundSpeed);
    }

    public sealed class PatternBuffer : IDisposable
    {
        internal IntPtr Handle { get; private set; }

        internal PatternBuffer(int numDevices)
        {
            var handle = NativePattern.autd3_pattern_buffer_new((UIntPtr)numDevices);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create pattern buffer");
            }
            Handle = handle;
        }



        public PatternBuffer(Geometry geometry) : this(geometry.NumDevices)
        {
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


        public static void Focus(Geometry geometry, Vector3 target, float wavelengthMm, Intensity intensity, PatternBuffer buffer)
        {
            var t = new[] { target.X, target.Y, target.Z };
            if (NativePattern.autd3_pattern_focus(geometry.Handle, t, wavelengthMm, intensity.Value, buffer.Handle) != 0)
            {
                throw new Autd3Exception("focus failed (buffer device count must match geometry)");
            }
        }


        public static void Null(PatternBuffer buffer) => NativePattern.autd3_pattern_null(buffer.Handle);
    }
}
