using System;
using System.Threading;
using System.Collections;
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

        private const string ClientLib = "autd3capi";

        static NativeModulation()
        {
            NativeAbi.Verify(Lib, autd3_abi_version());
            NativeAbi.Verify(ClientLib, autd3capi_abi_version());
        }

        [DllImport(Lib)]
        private static extern uint autd3_abi_version();

        [DllImport(ClientLib, EntryPoint = "autd3_abi_version")]
        private static extern uint autd3capi_abi_version();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_buffer_new();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_buffer_from_bytes(byte[] data, UIntPtr len);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_modulation_buffer_len(IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_buffer_get(IntPtr buffer, UIntPtr index, out byte @out);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_buffer_set(IntPtr buffer, UIntPtr index, byte value);

        [DllImport(Lib)]
        internal static extern void autd3_modulation_buffer_free(IntPtr buffer);

        [DllImport(Lib)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_modulation_samples_per_period(ushort divider, uint freqHz, out uint outValue);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_sine_option_new(byte amplitude, byte offset, float phase, [MarshalAs(UnmanagedType.I1)] bool clamp, IntPtr samplingConfig);

        [DllImport(Lib)]
        internal static extern void autd3_modulation_sine_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_sine(byte mode, float freq, uint freqInt, IntPtr option, IntPtr buffer, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_square_option_new(byte low, byte high, float duty, IntPtr samplingConfig);

        [DllImport(Lib)]
        internal static extern void autd3_modulation_square_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_square(byte mode, float freq, uint freqInt, IntPtr option, IntPtr buffer, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_modulation_fourier_option_new([MarshalAs(UnmanagedType.I1)] bool hasScaleFactor, float scaleFactor, [MarshalAs(UnmanagedType.I1)] bool clamp, byte offset);

        [DllImport(Lib)]
        internal static extern void autd3_modulation_fourier_option_free(IntPtr option);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_fourier(SineComponentNative[] components, UIntPtr numComponents, IntPtr option, IntPtr buffer, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_constant(byte intensity, IntPtr buffer);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_radiation_pressure(IntPtr src, IntPtr dst);

        [DllImport(Lib)]
        internal static extern int autd3_modulation_radiation_pressure_inplace(IntPtr buffer);

        [DllImport(ClientLib)]
        internal static extern IntPtr autd3_op_modulation(byte bank, IntPtr samplingConfig, IntPtr modulationBuffer, ushort loopRep, byte transitionMode, ulong transitionValue, uint transitionMarginNs);
    }

    public sealed class ModulationBuffer : IDisposable, IEnumerable<byte>
    {
        private IntPtr _handle;

        internal IntPtr Handle => _handle;

        private ModulationBuffer(IntPtr handle)
        {
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create modulation buffer");
            }
            _handle = handle;
        }

        internal ModulationBuffer() : this(NativeModulation.autd3_modulation_buffer_new())
        {
        }

        public ModulationBuffer(int length) : this(length == 0
            ? NativeModulation.autd3_modulation_buffer_new()
            : NativeModulation.autd3_modulation_buffer_from_bytes(new byte[length], (UIntPtr)length))
        {
        }

        public static ModulationBuffer FromBytes(byte[] data) =>
            new ModulationBuffer(NativeModulation.autd3_modulation_buffer_from_bytes(data, (UIntPtr)data.Length));

        public int Length => (int)NativeModulation.autd3_modulation_buffer_len(Handle);

        public byte this[int index]
        {
            get
            {
                if (NativeModulation.autd3_modulation_buffer_get(Handle, (UIntPtr)index, out var value) != 0)
                    throw new ArgumentOutOfRangeException(nameof(index));
                return value;
            }
            set
            {
                if (NativeModulation.autd3_modulation_buffer_set(Handle, (UIntPtr)index, value) != 0)
                    throw new ArgumentOutOfRangeException(nameof(index));
            }
        }

        public IEnumerator<byte> GetEnumerator()
        {
            var length = Length;
            for (var i = 0; i < length; i++)
            {
                yield return this[i];
            }
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        public void Dispose()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeModulation.autd3_modulation_buffer_free(handle);
            }
            GC.SuppressFinalize(this);
        }

        ~ModulationBuffer()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeModulation.autd3_modulation_buffer_free(handle);
            }
        }
    }

    public readonly struct SineOption
    {
        public byte Amplitude { get; }
        public byte Offset { get; }
        public Angle Phase { get; }
        public bool Clamp { get; }
        public SamplingConfig SamplingConfig { get; }

        public SineOption() : this(amplitude: 0xFF)
        {
        }

        public SineOption(byte amplitude = 0xFF, byte offset = 0x80, Angle phase = default, bool clamp = false, SamplingConfig? samplingConfig = null)
        {
            Amplitude = amplitude;
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

        public SquareOption() : this(low: 0x00)
        {
        }

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
        private readonly bool _nearest;

        public SineComponent(Freq freq, SineOption option)
        {
            Freq = freq;
            Option = option;
            _nearest = false;
        }

        public SineComponent(Nearest<Freq> freq, SineOption option)
        {
            Freq = freq.Value;
            Option = option;
            _nearest = true;
        }

        internal byte ModeCode => _nearest ? (byte)2 : Freq.ModeCode;
    }

    public sealed class Modulation : ICommand
    {
        private readonly ModulationBank _bank;
        private readonly SamplingConfig _samplingConfig;
        private readonly ModulationBuffer _buffer;
        private readonly LoopBehavior _loopBehavior;
        private readonly TransitionMode _transitionMode;

        public Modulation(SamplingConfig config, ModulationBuffer data, LoopBehavior? loopBehavior = null, TransitionMode? transitionMode = null)
            : this(ModulationBank.B0, config, data, loopBehavior, transitionMode)
        {
        }

        public Modulation(ModulationBank bank, SamplingConfig config, ModulationBuffer data, LoopBehavior? loopBehavior = null, TransitionMode? transitionMode = null)
        {
            _bank = bank;
            _samplingConfig = config;
            _buffer = data;
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
                    (byte)_bank, sampling, _buffer.Handle, _loopBehavior.Rep, _transitionMode.Mode, _transitionMode.Value, _transitionMode.MarginNs);
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(sampling);
            }
        }

        public static void Sine(Nearest<Freq> freq, SineOption option, ModulationBuffer dst) =>
            Sine(freq.Value, 2, option, dst);

        public static void Sine(Freq freq, SineOption option, ModulationBuffer dst) =>
            Sine(freq, freq.ModeCode, option, dst);

        private static void Sine(Freq freq, byte modeCode, SineOption option, ModulationBuffer dst)
        {
            var sampling = option.SamplingConfig.CreateHandle();
            try
            {
                var optionHandle = NativeModulation.autd3_modulation_sine_option_new(
                    option.Amplitude, option.Offset, option.Phase.Radian, option.Clamp, sampling);
                try
                {
                    var err = new byte[NativeAbi.ErrorBufferLength];
                    if (NativeModulation.autd3_modulation_sine(modeCode, freq.HzValue, freq.HzIntValue, optionHandle, dst.Handle, err, (UIntPtr)err.Length) != 0)
                    {
                        throw new Autd3Exception(NativeUtil.Utf8(err));
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

        public static void Square(Nearest<Freq> freq, SquareOption option, ModulationBuffer dst) =>
            Square(freq.Value, 2, option, dst);

        public static void Square(Freq freq, SquareOption option, ModulationBuffer dst) =>
            Square(freq, freq.ModeCode, option, dst);

        private static void Square(Freq freq, byte modeCode, SquareOption option, ModulationBuffer dst)
        {
            var sampling = option.SamplingConfig.CreateHandle();
            try
            {
                var optionHandle = NativeModulation.autd3_modulation_square_option_new(
                    option.Low, option.High, option.Duty, sampling);
                try
                {
                    var err = new byte[NativeAbi.ErrorBufferLength];
                    if (NativeModulation.autd3_modulation_square(modeCode, freq.HzValue, freq.HzIntValue, optionHandle, dst.Handle, err, (UIntPtr)err.Length) != 0)
                    {
                        throw new Autd3Exception(NativeUtil.Utf8(err));
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

        public static void Constant(byte intensity, ModulationBuffer dst)
        {
            if (NativeModulation.autd3_modulation_constant(intensity, dst.Handle) != 0)
            {
                throw new Autd3Exception("constant modulation failed");
            }
        }

        public static void Fourier(SineComponent[] components, FourierOption option, ModulationBuffer dst)
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
                        c.Option.Amplitude, c.Option.Offset, c.Option.Phase.Radian, c.Option.Clamp, sampling);
                    optionHandles.Add(optionHandle);
                    native[i] = new SineComponentNative
                    {
                        Mode = c.ModeCode,
                        Freq = c.Freq.HzValue,
                        FreqInt = c.Freq.HzIntValue,
                        Option = optionHandle,
                    };
                }

                var fourierOption = NativeModulation.autd3_modulation_fourier_option_new(
                    option.ScaleFactor.HasValue, option.ScaleFactor ?? 0f, option.Clamp, option.Offset);
                try
                {
                    var err = new byte[NativeAbi.ErrorBufferLength];
                    if (NativeModulation.autd3_modulation_fourier(native, (UIntPtr)components.Length, fourierOption, dst.Handle, err, (UIntPtr)err.Length) != 0)
                    {
                        throw new Autd3Exception(NativeUtil.Utf8(err));
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
