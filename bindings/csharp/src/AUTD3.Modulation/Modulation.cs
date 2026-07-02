using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace AUTD3
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct SineComponentNative
    {
        public byte Mode;
        public float Freq;
        public uint FreqInt;
        public IntPtr Option;
    }

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
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_modulation_samples_per_period(ushort divider, uint freqHz, out uint outValue);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_sine_option_new(byte intensity, byte offset, float phase, [MarshalAs(UnmanagedType.I1)] bool clamp, IntPtr samplingConfig);

        [DllImport(Lib)]
        internal static extern void autd3_modulation_sine_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_sine(byte mode, float freq, uint freqInt, IntPtr option, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_square_option_new(byte low, byte high, float duty, IntPtr samplingConfig);

        [DllImport(Lib)]
        internal static extern void autd3_modulation_square_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_square(byte mode, float freq, uint freqInt, IntPtr option, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_fourier_option_new([MarshalAs(UnmanagedType.I1)] bool hasScaleFactor, float scaleFactor, [MarshalAs(UnmanagedType.I1)] bool clamp, byte offset);

        [DllImport(Lib)]
        internal static extern void autd3_modulation_fourier_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_fourier(SineComponentNative[] components, UIntPtr numComponents, IntPtr option, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_constant(byte intensity, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_radiation_pressure(IntPtr src, IntPtr dst);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_radiation_pressure_inplace(IntPtr buffer);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_modulation(byte bank, IntPtr samplingConfig, IntPtr modulationBuffer, ushort loopRep, byte transitionMode, ulong transitionValue);
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

        internal ModulationBuffer() : this(NativeModulation.autd3_modulation_buffer_new())
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
        public Angle Phase { get; }
        public bool Clamp { get; }
        public SamplingConfig SamplingConfig { get; }

        public SineOption(byte intensity = 0xFF, byte offset = 0x80, Angle phase = default, bool clamp = false, SamplingConfig? samplingConfig = null)
        {
            Intensity = intensity;
            Offset = offset;
            Phase = phase;
            Clamp = clamp;
            SamplingConfig = samplingConfig ?? SamplingConfig.Freq4k;
        }
    }

    public readonly struct SquareOption
    {
        public byte Low { get; }
        public byte High { get; }
        public float Duty { get; }
        public SamplingConfig SamplingConfig { get; }

        public SquareOption(byte low = 0x00, byte high = 0xFF, float duty = 0.5f, SamplingConfig? samplingConfig = null)
        {
            Low = low;
            High = high;
            Duty = duty;
            SamplingConfig = samplingConfig ?? SamplingConfig.Freq4k;
        }
    }

    public readonly struct FourierOption
    {
        public float? ScaleFactor { get; }
        public bool Clamp { get; }
        public byte Offset { get; }

        public FourierOption(float? scaleFactor = null, bool clamp = false, byte offset = 0x00)
        {
            ScaleFactor = scaleFactor;
            Clamp = clamp;
            Offset = offset;
        }
    }

    public readonly struct SineComponent
    {
        public Freq Freq { get; }
        public SineOption Option { get; }

        public SineComponent(Freq freq, SineOption option)
        {
            Freq = freq;
            Option = option;
        }
    }

    public sealed class Modulation : ICommand
    {
        private readonly ModulationBank _bank;
        private readonly SamplingConfig _samplingConfig;
        private readonly ModulationBuffer _buffer;
        private readonly LoopBehavior _loopBehavior;
        private readonly TransitionMode _transitionMode;

        public Modulation(SamplingConfig samplingConfig, ModulationBuffer buffer, ModulationBank bank = ModulationBank.B0, LoopBehavior? loopBehavior = null, TransitionMode? transitionMode = null)
        {
            _bank = bank;
            _samplingConfig = samplingConfig;
            _buffer = buffer;
            _loopBehavior = loopBehavior ?? LoopBehavior.Infinite;
            _transitionMode = transitionMode ?? TransitionMode.Immediate;
        }

        public static ModulationBuffer ModulationBuffer() => new ModulationBuffer();

        public static uint SamplesPerPeriod(ushort divider, uint freqHz)
        {
            if (!NativeModulation.autd3_modulation_samples_per_period(divider, freqHz, out var value))
            {
                throw new Autd3Exception("samples_per_period is not an integer for the given frequency");
            }
            return value;
        }

        IntPtr ICommand.CreateOp()
        {
            var sampling = _samplingConfig.CreateHandle();
            try
            {
                return NativeModulation.autd3_op_modulation(
                    (byte)_bank, sampling, _buffer.Handle, _loopBehavior.Rep, _transitionMode.Mode, _transitionMode.Value);
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(sampling);
            }
        }

        public static void Sine(Freq freq, SineOption option, ModulationBuffer buffer)
        {
            var sampling = option.SamplingConfig.CreateHandle();
            try
            {
                var optionHandle = NativeModulation.autd3_modulation_sine_option_new(
                    option.Intensity, option.Offset, option.Phase.Radian, option.Clamp, sampling);
                try
                {
                    if (NativeModulation.autd3_modulation_sine(freq.ModeCode, freq.HzValue, freq.HzIntValue, optionHandle, buffer.Handle) != 0)
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

        public static void Square(Freq freq, SquareOption option, ModulationBuffer buffer)
        {
            var sampling = option.SamplingConfig.CreateHandle();
            try
            {
                var optionHandle = NativeModulation.autd3_modulation_square_option_new(
                    option.Low, option.High, option.Duty, sampling);
                try
                {
                    if (NativeModulation.autd3_modulation_square(freq.ModeCode, freq.HzValue, freq.HzIntValue, optionHandle, buffer.Handle) != 0)
                    {
                        throw new Autd3Exception("square modulation failed");
                    }
                }
                finally
                {
                    NativeModulation.autd3_modulation_square_option_free(optionHandle);
                }
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(sampling);
            }
        }

        public static void Constant(byte intensity, ModulationBuffer buffer)
        {
            if (NativeModulation.autd3_modulation_constant(intensity, buffer.Handle) != 0)
            {
                throw new Autd3Exception("constant modulation failed");
            }
        }

        public static void Fourier(SineComponent[] components, FourierOption option, ModulationBuffer buffer)
        {
            var samplingHandles = new List<IntPtr>();
            var optionHandles = new List<IntPtr>();
            var native = new SineComponentNative[components.Length];
            try
            {
                for (var i = 0; i < components.Length; i++)
                {
                    var c = components[i];
                    var sampling = c.Option.SamplingConfig.CreateHandle();
                    samplingHandles.Add(sampling);
                    var optionHandle = NativeModulation.autd3_modulation_sine_option_new(
                        c.Option.Intensity, c.Option.Offset, c.Option.Phase.Radian, c.Option.Clamp, sampling);
                    optionHandles.Add(optionHandle);
                    native[i] = new SineComponentNative
                    {
                        Mode = c.Freq.ModeCode,
                        Freq = c.Freq.HzValue,
                        FreqInt = c.Freq.HzIntValue,
                        Option = optionHandle,
                    };
                }

                var fourierOption = NativeModulation.autd3_modulation_fourier_option_new(
                    option.ScaleFactor.HasValue, option.ScaleFactor ?? 0f, option.Clamp, option.Offset);
                try
                {
                    if (NativeModulation.autd3_modulation_fourier(native, (UIntPtr)components.Length, fourierOption, buffer.Handle) != 0)
                    {
                        throw new Autd3Exception("fourier modulation failed");
                    }
                }
                finally
                {
                    NativeModulation.autd3_modulation_fourier_option_free(fourierOption);
                }
            }
            finally
            {
                foreach (var optionHandle in optionHandles)
                {
                    NativeModulation.autd3_modulation_sine_option_free(optionHandle);
                }
                foreach (var sampling in samplingHandles)
                {
                    NativeCore.autd3_core_sampling_config_free(sampling);
                }
            }
        }

        public static void RadiationPressure(ModulationBuffer src, ModulationBuffer dst)
        {
            if (NativeModulation.autd3_modulation_radiation_pressure(src.Handle, dst.Handle) != 0)
            {
                throw new Autd3Exception("radiation pressure failed");
            }
        }

        public static void RadiationPressureInplace(ModulationBuffer buffer)
        {
            if (NativeModulation.autd3_modulation_radiation_pressure_inplace(buffer.Handle) != 0)
            {
                throw new Autd3Exception("radiation pressure failed");
            }
        }
    }
}
