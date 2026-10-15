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

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [DllImport(ClientLib, EntryPoint = "autd3_abi_version", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3capi_abi_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_modulation_buffer_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_modulation_buffer_from_bytes(byte[] data, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_modulation_buffer_len(ModulationBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_buffer_get(ModulationBufferHandle buffer, UIntPtr index, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_buffer_set(ModulationBufferHandle buffer, UIntPtr index, byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_modulation_buffer_free(IntPtr buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_modulation_samples_per_period(ushort divider, uint freqHz, out uint outValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_modulation_sine_option_new(byte amplitude, byte offset, float phase, [MarshalAs(UnmanagedType.I1)] bool clamp, IntPtr samplingConfig);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_modulation_sine_option_free(IntPtr option);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_sine_option_default(out byte amplitude, out byte offset, out float phaseRad, [MarshalAs(UnmanagedType.I1)] out bool clamp, out ushort samplingDivide);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_square_option_default(out byte low, out byte high, out float duty, out ushort samplingDivide);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_fourier_option_default([MarshalAs(UnmanagedType.I1)] out bool hasScaleFactor, out float scaleFactor, [MarshalAs(UnmanagedType.I1)] out bool clamp, out byte offset);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_sine(byte mode, float freq, uint freqInt, IntPtr option, ModulationBufferHandle buffer, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_modulation_square_option_new(byte low, byte high, float duty, IntPtr samplingConfig);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_modulation_square_option_free(IntPtr option);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_square(byte mode, float freq, uint freqInt, IntPtr option, ModulationBufferHandle buffer, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_modulation_fourier_option_new([MarshalAs(UnmanagedType.I1)] bool hasScaleFactor, float scaleFactor, [MarshalAs(UnmanagedType.I1)] bool clamp, byte offset);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_modulation_fourier_option_free(IntPtr option);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_fourier(SineComponentNative[] components, UIntPtr numComponents, IntPtr option, ModulationBufferHandle buffer, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_buffer_copy_to(ModulationBufferHandle buffer, [Out] byte[] dst, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_constant(byte amplitude, ModulationBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_radiation_pressure(ModulationBufferHandle src, ModulationBufferHandle dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_radiation_pressure_inplace(ModulationBufferHandle buffer);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_modulation(byte bank, IntPtr samplingConfig, ModulationBufferHandle modulationBuffer, ushort loopRep, byte transitionMode, ulong transitionValue);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_write_modulation_buffer(byte bank, uint offset, ModulationBufferHandle modulationBuffer);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_config_modulation(byte bank, IntPtr samplingConfig, uint size, ushort rep);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_activate_modulation_bank(byte bank, byte transitionMode, ulong transitionValue);
    }

    internal sealed class ModulationBufferHandle : Autd3SafeHandle
    {
        internal ModulationBufferHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeModulation.autd3_modulation_buffer_free(handle);
            return true;
        }
    }

    public sealed class ModulationBuffer : IDisposable, IEnumerable<byte>
    {
        private readonly ModulationBufferHandle _handle;

        internal ModulationBufferHandle Handle => _handle;

        private ModulationBuffer(IntPtr handle)
        {
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create modulation buffer");
            }
            _handle = new ModulationBufferHandle(handle);
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

        public byte[] ToArray()
        {
            var data = new byte[Length];
            if (NativeModulation.autd3_modulation_buffer_copy_to(Handle, data, (UIntPtr)data.Length) != 0)
            {
                throw new Autd3Exception("failed to copy the modulation buffer");
            }
            return data;
        }

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

        public void Dispose() => _handle.Dispose();
    }

    internal static class ModulationOptionDefaults
    {
        internal static readonly byte SineAmplitude;
        internal static readonly byte SineOffset;
        internal static readonly Angle SinePhase;
        internal static readonly bool SineClamp;
        internal static readonly SamplingConfig SineSamplingConfig;
        internal static readonly byte SquareLow;
        internal static readonly byte SquareHigh;
        internal static readonly float SquareDuty;
        internal static readonly SamplingConfig SquareSamplingConfig;
        internal static readonly float? FourierScaleFactor;
        internal static readonly bool FourierClamp;
        internal static readonly byte FourierOffset;

        static ModulationOptionDefaults()
        {
            OptionNative.Apply("preset", NativeModulation.autd3_modulation_sine_option_default(out SineAmplitude, out SineOffset, out var phase, out SineClamp, out var sineDivide));
            SinePhase = Angle.FromRad(phase);
            SineSamplingConfig = new SamplingConfig(sineDivide);
            OptionNative.Apply("preset", NativeModulation.autd3_modulation_square_option_default(out SquareLow, out SquareHigh, out SquareDuty, out var squareDivide));
            SquareSamplingConfig = new SamplingConfig(squareDivide);
            OptionNative.Apply("preset", NativeModulation.autd3_modulation_fourier_option_default(out var hasScaleFactor, out var scaleFactor, out FourierClamp, out FourierOffset));
            FourierScaleFactor = hasScaleFactor ? scaleFactor : (float?)null;
        }
    }

    public readonly struct SineOption
    {
        private readonly byte? _amplitude;
        public byte Amplitude { get => _amplitude ?? ModulationOptionDefaults.SineAmplitude; init => _amplitude = value; }
        private readonly byte? _offset;
        public byte Offset { get => _offset ?? ModulationOptionDefaults.SineOffset; init => _offset = value; }
        private readonly Angle? _phase;
        public Angle Phase { get => _phase ?? ModulationOptionDefaults.SinePhase; init => _phase = value; }
        private readonly bool? _clamp;
        public bool Clamp { get => _clamp ?? ModulationOptionDefaults.SineClamp; init => _clamp = value; }
        private readonly SamplingConfig? _samplingConfig;
        public SamplingConfig SamplingConfig { get => _samplingConfig ?? ModulationOptionDefaults.SineSamplingConfig; init => _samplingConfig = value; }
    }

    public readonly struct SquareOption
    {
        private readonly byte? _low;
        public byte Low { get => _low ?? ModulationOptionDefaults.SquareLow; init => _low = value; }
        private readonly byte? _high;
        public byte High { get => _high ?? ModulationOptionDefaults.SquareHigh; init => _high = value; }
        private readonly float? _duty;
        public float Duty { get => _duty ?? ModulationOptionDefaults.SquareDuty; init => _duty = value; }
        private readonly SamplingConfig? _samplingConfig;
        public SamplingConfig SamplingConfig { get => _samplingConfig ?? ModulationOptionDefaults.SquareSamplingConfig; init => _samplingConfig = value; }
    }

    public readonly struct FourierOption
    {
        private readonly bool _scaleFactorSet;
        private readonly float? _scaleFactor;

        public float? ScaleFactor
        {
            get => _scaleFactorSet ? _scaleFactor : ModulationOptionDefaults.FourierScaleFactor;
            init
            {
                _scaleFactor = value;
                _scaleFactorSet = true;
            }
        }

        private readonly bool? _clamp;
        public bool Clamp { get => _clamp ?? ModulationOptionDefaults.FourierClamp; init => _clamp = value; }
        private readonly byte? _offset;
        public byte Offset { get => _offset ?? ModulationOptionDefaults.FourierOffset; init => _offset = value; }
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

        public bool IsNearest => _nearest;

        internal byte ModeCode => _nearest ? (byte)2 : Freq.ModeCode;
    }

    public sealed class Modulation : ICommand
    {
        private readonly ModulationBank _bank;
        private readonly SamplingConfig _samplingConfig;
        private readonly ModulationBuffer _buffer;
        private readonly LoopBehavior _loopBehavior;
        private readonly TransitionMode _transitionMode;

        public Modulation(SamplingConfig config, ModulationBuffer data, LoopBehavior loopBehavior = default, TransitionMode transitionMode = default)
            : this(ModulationBank.B0, config, data, loopBehavior, transitionMode)
        {
        }

        public Modulation(ModulationBank bank, SamplingConfig config, ModulationBuffer data, LoopBehavior loopBehavior = default, TransitionMode transitionMode = default)
        {
            _bank = bank;
            _samplingConfig = config;
            _buffer = data;
            _loopBehavior = loopBehavior;
            _transitionMode = transitionMode;
        }

        public static ModulationBuffer ModulationBuffer() => new ModulationBuffer();

        public static uint? SamplesPerPeriod(ushort divider, Freq freq)
        {
            if (freq.Mode != AUTD3.Freq.FreqMode.IntExact)
            {
                throw new Autd3Exception("samples_per_period requires an integer frequency", Autd3ErrorCode.InvalidArgument);
            }
            return NativeModulation.autd3_modulation_samples_per_period(divider, freq.HzIntValue, out var value) ? value : null;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
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
                    option.Amplitude, option.Offset, option.Phase.Rad, option.Clamp, sampling);
                try
                {
                    var err = new byte[NativeAbi.ErrorBufferLength];
                    var code = NativeModulation.autd3_modulation_sine(modeCode, freq.HzValue, freq.HzIntValue, optionHandle, dst.Handle, err, (UIntPtr)err.Length);
                    if (code != 0)
                    {
                        throw Autd3Exception.FromNative(code, err);
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
                    var code = NativeModulation.autd3_modulation_square(modeCode, freq.HzValue, freq.HzIntValue, optionHandle, dst.Handle, err, (UIntPtr)err.Length);
                    if (code != 0)
                    {
                        throw Autd3Exception.FromNative(code, err);
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

        public static void Constant(byte amplitude, ModulationBuffer dst)
        {
            if (NativeModulation.autd3_modulation_constant(amplitude, dst.Handle) != 0)
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
                        c.Option.Amplitude, c.Option.Offset, c.Option.Phase.Rad, c.Option.Clamp, sampling);
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
                    var code = NativeModulation.autd3_modulation_fourier(native, (UIntPtr)components.Length, fourierOption, dst.Handle, err, (UIntPtr)err.Length);
                    if (code != 0)
                    {
                        throw Autd3Exception.FromNative(code, err);
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
            if (ReferenceEquals(src, dst))
            {
                throw new Autd3Exception("src and dst must be different buffers; use RadiationPressureInplace to transform a buffer in place", Autd3ErrorCode.InvalidArgument);
            }
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
