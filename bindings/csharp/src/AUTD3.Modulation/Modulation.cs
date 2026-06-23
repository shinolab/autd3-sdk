using System;
using System.Runtime.InteropServices;

namespace AUTD3
{
    internal static class NativeModulation
    {
        private const string Lib = "autd3_modulation";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_buffer_new();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_buffer_from_bytes(byte[] data, UIntPtr len);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_modulation_buffer_len(IntPtr buffer);

        [DllImport(Lib)]
        internal static extern void autd3_modulation_buffer_free(IntPtr buffer);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_sine_option_new(byte intensity, byte offset, float phase, [MarshalAs(UnmanagedType.I1)] bool clamp, IntPtr samplingConfig);

        [DllImport(Lib)]
        internal static extern void autd3_modulation_sine_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_sine(float freq, IntPtr option, IntPtr buffer);

        [DllImport("autd3")]
        internal static extern IntPtr autd3_op_modulation(IntPtr samplingConfig, IntPtr modulationBuffer);
    }

    public sealed class ModulationBuffer : IDisposable
    {
        internal IntPtr Handle { get; private set; }

        private ModulationBuffer(IntPtr handle)
        {
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create modulation buffer");
            }
            Handle = handle;
        }

        public ModulationBuffer() : this(NativeModulation.autd3_modulation_buffer_new())
        {
        }

        public static ModulationBuffer FromBytes(byte[] data) =>
            new ModulationBuffer(NativeModulation.autd3_modulation_buffer_from_bytes(data, (UIntPtr)data.Length));

        public int Length => (int)NativeModulation.autd3_modulation_buffer_len(Handle);

        public void Dispose()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeModulation.autd3_modulation_buffer_free(Handle);
                Handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~ModulationBuffer()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeModulation.autd3_modulation_buffer_free(Handle);
            }
        }
    }

    public readonly struct SineOption
    {
        public byte Intensity { get; }
        public byte Offset { get; }
        public float Phase { get; }
        public bool Clamp { get; }
        public SamplingConfig SamplingConfig { get; }

        public SineOption(byte intensity = 0xFF, byte offset = 0x80, float phase = 0f, bool clamp = false, SamplingConfig samplingConfig = default)
        {
            Intensity = intensity;
            Offset = offset;
            Phase = phase;
            Clamp = clamp;
            SamplingConfig = samplingConfig;
        }
    }

    public sealed class Modulation : ICommand
    {
        private readonly SamplingConfig _samplingConfig;
        private readonly ModulationBuffer _buffer;

        public Modulation(SamplingConfig samplingConfig, ModulationBuffer buffer)
        {
            _samplingConfig = samplingConfig;
            _buffer = buffer;
        }

        IntPtr ICommand.CreateOp()
        {
            var sampling = _samplingConfig.CreateHandle();
            try
            {
                return NativeModulation.autd3_op_modulation(sampling, _buffer.Handle);
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(sampling);
            }
        }

        public static void Sine(float freq, SineOption option, ModulationBuffer buffer)
        {
            var sampling = option.SamplingConfig.CreateHandle();
            try
            {
                var optionHandle = NativeModulation.autd3_modulation_sine_option_new(
                    option.Intensity, option.Offset, option.Phase, option.Clamp, sampling);
                try
                {
                    if (NativeModulation.autd3_modulation_sine(freq, optionHandle, buffer.Handle) != 0)
                    {
                        throw new Autd3Exception("sine modulation failed");
                    }
                }
                finally
                {
                    NativeModulation.autd3_modulation_sine_option_free(optionHandle);
                }
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(sampling);
            }
        }
    }
}
