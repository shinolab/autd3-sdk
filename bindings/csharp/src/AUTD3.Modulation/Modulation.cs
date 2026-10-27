using System;
using System.Threading;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace AUTD3
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct SineOptionNative
    {
        public byte Amplitude;
        public byte Offset;
        public float PhaseRad;
        public byte Clamp;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SquareOptionNative
    {
        public byte Low;
        public byte High;
        public float Duty;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct FourierOptionNative
    {
        public byte HasScaleFactor;
        public float ScaleFactor;
        public byte Clamp;
        public byte Offset;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SineComponentNative
    {
        public float Freq;
        public SineOptionNative Option;
    }

    internal static class NativeModulation
    {
        private const string Lib = "autd3_modulation";

        static NativeModulation() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

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
        internal static extern bool autd3_modulation_samples_per_period(ushort divider, float freqHz, out uint outValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_sine_option_default(out byte amplitude, out byte offset, out float phaseRad, [MarshalAs(UnmanagedType.I1)] out bool clamp);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_square_option_default(out byte low, out byte high, out float duty);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_fourier_option_default([MarshalAs(UnmanagedType.I1)] out bool hasScaleFactor, out float scaleFactor, [MarshalAs(UnmanagedType.I1)] out bool clamp, out byte offset);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_sine(float freq, SineOptionNative option, ushort samplingDivide, ModulationBufferHandle buffer, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_square(float freq, SquareOptionNative option, ushort samplingDivide, ModulationBufferHandle buffer, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_fourier(SineComponentNative[] components, UIntPtr numComponents, FourierOptionNative option, ushort samplingDivide, ModulationBufferHandle buffer, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_buffer_copy_to(ModulationBufferHandle buffer, [Out] byte[] dst, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_constant(byte amplitude, ModulationBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_modulation_radiation_pressure_inplace(ModulationBufferHandle buffer);
    }

    internal static class NativeModulationOp
    {
        private const string Lib = "autd3capi";

        static NativeModulationOp() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_modulation(byte bank, ushort samplingDivide, ModulationBufferHandle modulationBuffer, ushort loopRep, byte transitionMode, ulong transitionValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_write_modulation_buffer(byte bank, uint offset, ModulationBufferHandle modulationBuffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_op_write_modulation_buffer_max_len();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_config_modulation(byte bank, ushort samplingDivide, uint size, ushort rep);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
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
        internal static readonly byte SquareLow;
        internal static readonly byte SquareHigh;
        internal static readonly float SquareDuty;
        internal static readonly float? FourierScaleFactor;
        internal static readonly bool FourierClamp;
        internal static readonly byte FourierOffset;

        static ModulationOptionDefaults()
        {
            OptionNative.Apply("preset", NativeModulation.autd3_modulation_sine_option_default(out SineAmplitude, out SineOffset, out var phase, out SineClamp));
            SinePhase = Angle.FromRad(phase);
            OptionNative.Apply("preset", NativeModulation.autd3_modulation_square_option_default(out SquareLow, out SquareHigh, out SquareDuty));
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

        internal SineOptionNative ToNative() => new SineOptionNative
        {
            Amplitude = Amplitude,
            Offset = Offset,
            PhaseRad = Phase.Rad,
            Clamp = OptionNative.Flag(Clamp),
        };
    }

    public readonly struct SquareOption
    {
        private readonly byte? _low;
        public byte Low { get => _low ?? ModulationOptionDefaults.SquareLow; init => _low = value; }
        private readonly byte? _high;
        public byte High { get => _high ?? ModulationOptionDefaults.SquareHigh; init => _high = value; }
        private readonly float? _duty;
        public float Duty { get => _duty ?? ModulationOptionDefaults.SquareDuty; init => _duty = value; }

        internal SquareOptionNative ToNative() => new SquareOptionNative
        {
            Low = Low,
            High = High,
            Duty = Duty,
        };
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

        internal FourierOptionNative ToNative() => new FourierOptionNative
        {
            HasScaleFactor = OptionNative.Flag(ScaleFactor.HasValue),
            ScaleFactor = ScaleFactor ?? 0f,
            Clamp = OptionNative.Flag(Clamp),
            Offset = Offset,
        };
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

        public static uint? SamplesPerPeriod(ushort divider, Freq freq) =>
            NativeModulation.autd3_modulation_samples_per_period(divider, freq.Hz, out var value) ? value : null;

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            NativeModulationOp.autd3_op_modulation(
                (byte)_bank, _samplingConfig.DivideForNative(), _buffer.Handle, _loopBehavior.Rep, _transitionMode.Mode, _transitionMode.Value);

        public static void Sine(Freq freq, SineOption option, SamplingConfig config, ModulationBuffer dst)
        {
            var sampling = config.DivideForNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeModulation.autd3_modulation_sine(freq.Hz, option.ToNative(), sampling, dst.Handle, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
        }

        public static void Square(Freq freq, SquareOption option, SamplingConfig config, ModulationBuffer dst)
        {
            var sampling = config.DivideForNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeModulation.autd3_modulation_square(freq.Hz, option.ToNative(), sampling, dst.Handle, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
        }

        public static void Constant(byte amplitude, ModulationBuffer dst)
        {
            if (NativeModulation.autd3_modulation_constant(amplitude, dst.Handle) != 0)
            {
                throw new Autd3Exception("constant modulation failed");
            }
        }

        public static void Fourier(SineComponent[] components, FourierOption option, SamplingConfig config, ModulationBuffer dst)
        {
            var sampling = config.DivideForNative();
            var native = new SineComponentNative[components.Length];
            for (var i = 0; i < components.Length; i++)
            {
                var c = components[i];
                native[i] = new SineComponentNative
                {
                    Freq = c.Freq.Hz,
                    Option = c.Option.ToNative(),
                };
            }
            var err = new byte[NativeAbi.ErrorBufferLength];
            var code = NativeModulation.autd3_modulation_fourier(native, (UIntPtr)components.Length, option.ToNative(), sampling, dst.Handle, err, (UIntPtr)err.Length);
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
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
