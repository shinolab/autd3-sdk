using System;
using System.Runtime.InteropServices;

namespace AUTD3
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct GpioOutNative
    {
        public byte Kind;
        public ulong Value;
    }

    internal static class NativeCommand
    {
        private const string Lib = "autd3capi";

        static NativeCommand() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_clear();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_synchronize();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_nop();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_force_fan([MarshalAs(UnmanagedType.I1)] bool value);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_set_silencer_completion_time(ulong intensityNs, ulong phaseNs, [MarshalAs(UnmanagedType.I1)] bool strict);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_set_silencer_update_rate(ushort intensity, ushort phase);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_set_silencer_disable();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_set_gpio_out(GpioOutNative[] outputs);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_emulate_gpio_in(byte[] values);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_set_output_mask(byte[] masks, UIntPtr[] lens, UIntPtr numDevices);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_set_phase_correction(byte[] phases, UIntPtr[] lens, UIntPtr numDevices);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_set_pulse_width_table(ushort[] table);

        [DllImport(Lib)]
        internal static extern void autd3_set_pulse_width_table_default_table([Out] ushort[] outTable);

        [DllImport(Lib)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_pulse_width_from_duty(float duty, [Out] ushort[] outValue);

        [DllImport(Lib)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_pulse_width_new(ushort pulseWidth, [Out] ushort[] outValue);
    }

    public readonly struct GpioOut
    {
        internal byte Kind { get; }
        internal ulong Value { get; }

        private GpioOut(byte kind, ulong value)
        {
            Kind = kind;
            Value = value;
        }

        public static GpioOut Off => new GpioOut(0, 0);
        public static GpioOut BaseSignal => new GpioOut(1, 0);
        public static GpioOut Thermo => new GpioOut(2, 0);
        public static GpioOut ForceFan => new GpioOut(3, 0);
        public static GpioOut Sync => new GpioOut(4, 0);
        public static GpioOut ModBank => new GpioOut(5, 0);
        public static GpioOut ModIdx(ushort idx) => new GpioOut(6, idx);
        public static GpioOut PatternBank => new GpioOut(7, 0);
        public static GpioOut PatternIdx(ushort idx) => new GpioOut(8, idx);
        public static GpioOut IsStmMode => new GpioOut(9, 0);
        public static GpioOut SysTimeEq(DcSysTime sysTime) => new GpioOut(10, sysTime.SysTime);
        public static GpioOut SyncDiff => new GpioOut(11, 0);
        public static GpioOut PwmOut(byte transducer) => new GpioOut(12, transducer);
        public static GpioOut Direct(bool on) => new GpioOut(13, on ? 1UL : 0UL);

        internal GpioOutNative ToNative() => new GpioOutNative { Kind = Kind, Value = Value };
    }

    public sealed class Clear : ICommand
    {
        IntPtr ICommand.CreateOp() => NativeCommand.autd3_op_clear();
    }

    public sealed class Synchronize : ICommand
    {
        IntPtr ICommand.CreateOp() => NativeCommand.autd3_op_synchronize();
    }

    public sealed class Nop : ICommand
    {
        IntPtr ICommand.CreateOp() => NativeCommand.autd3_op_nop();
    }

    public sealed class ForceFan : ICommand
    {
        private readonly bool _value;

        public ForceFan(bool value)
        {
            _value = value;
        }

        IntPtr ICommand.CreateOp() => NativeCommand.autd3_op_force_fan(_value);
    }

    public interface ISilencerConfig
    {
        internal IntPtr CreateOp();
    }

    public sealed class FixedCompletionTime : ISilencerConfig
    {
        public TimeSpan Intensity { get; }
        public TimeSpan Phase { get; }
        public bool StrictMode { get; }

        public FixedCompletionTime(TimeSpan? intensity = null, TimeSpan? phase = null, bool strictMode = true)
        {
            // defaults: intensity = 25us * 10 = 250us, phase = 25us * 40 = 1000us
            Intensity = intensity ?? TimeSpan.FromTicks(2500);
            Phase = phase ?? TimeSpan.FromTicks(10000);
            StrictMode = strictMode;
        }

        IntPtr ISilencerConfig.CreateOp() =>
            NativeCommand.autd3_op_set_silencer_completion_time(
                (ulong)(Intensity.Ticks * 100), (ulong)(Phase.Ticks * 100), StrictMode);
    }

    public sealed class FixedUpdateRate : ISilencerConfig
    {
        public ushort Intensity { get; }
        public ushort Phase { get; }

        public FixedUpdateRate(ushort intensity = 256, ushort phase = 256)
        {
            Intensity = intensity;
            Phase = phase;
        }

        IntPtr ISilencerConfig.CreateOp()
        {
            var op = NativeCommand.autd3_op_set_silencer_update_rate(Intensity, Phase);
            if (op == IntPtr.Zero)
            {
                throw new Autd3Exception("silencer update rate must be >= 1");
            }
            return op;
        }
    }

    public sealed class SetSilencer : ICommand
    {
        private readonly ISilencerConfig? _config;
        private readonly bool _disable;

        public SetSilencer() : this(new FixedCompletionTime())
        {
        }

        public SetSilencer(ISilencerConfig config)
        {
            _config = config;
        }

        private SetSilencer(bool disable)
        {
            _disable = disable;
        }

        public static SetSilencer Disable() => new SetSilencer(true);

        IntPtr ICommand.CreateOp() =>
            _disable ? NativeCommand.autd3_op_set_silencer_disable() : _config!.CreateOp();
    }

    public sealed class SetGpioOut : ICommand
    {
        private readonly GpioOut[] _outputs;

        public SetGpioOut(GpioOut[] outputs)
        {
            if (outputs.Length != 4)
            {
                throw new Autd3Exception("SetGpioOut requires exactly 4 outputs");
            }
            _outputs = outputs;
        }

        IntPtr ICommand.CreateOp()
        {
            var native = new GpioOutNative[4];
            for (var i = 0; i < 4; i++)
            {
                native[i] = _outputs[i].ToNative();
            }
            return NativeCommand.autd3_op_set_gpio_out(native);
        }
    }

    public sealed class EmulateGpioIn : ICommand
    {
        private readonly bool[] _values;

        public EmulateGpioIn(bool[] values)
        {
            if (values.Length != 4)
            {
                throw new Autd3Exception("EmulateGpioIn requires exactly 4 values");
            }
            _values = values;
        }

        IntPtr ICommand.CreateOp()
        {
            var bytes = new byte[4];
            for (var i = 0; i < 4; i++)
            {
                bytes[i] = (byte)(_values[i] ? 1 : 0);
            }
            return NativeCommand.autd3_op_emulate_gpio_in(bytes);
        }
    }

    public sealed class SetOutputMask : ICommand
    {
        private readonly bool[][] _masks;

        public SetOutputMask(bool[][] masks)
        {
            _masks = masks;
        }

        IntPtr ICommand.CreateOp()
        {
            var lens = new UIntPtr[_masks.Length];
            var total = 0;
            for (var d = 0; d < _masks.Length; d++)
            {
                lens[d] = (UIntPtr)_masks[d].Length;
                total += _masks[d].Length;
            }
            var flat = new byte[total];
            var offset = 0;
            for (var d = 0; d < _masks.Length; d++)
            {
                for (var t = 0; t < _masks[d].Length; t++)
                {
                    flat[offset + t] = (byte)(_masks[d][t] ? 1 : 0);
                }
                offset += _masks[d].Length;
            }
            return NativeCommand.autd3_op_set_output_mask(flat, lens, (UIntPtr)_masks.Length);
        }
    }

    public readonly struct PulseWidth
    {
        public ushort Value { get; }

        public PulseWidth(ushort pulseWidth)
        {
            var outValue = new ushort[1];
            if (!NativeCommand.autd3_pulse_width_new(pulseWidth, outValue))
            {
                throw new Autd3Exception("invalid pulse width");
            }
            Value = outValue[0];
        }

        private PulseWidth(ushort value, bool validated)
        {
            _ = validated;
            Value = value;
        }

        internal static PulseWidth FromValidated(ushort value) => new PulseWidth(value, true);

        public static PulseWidth FromDuty(float duty)
        {
            var outValue = new ushort[1];
            if (!NativeCommand.autd3_pulse_width_from_duty(duty, outValue))
            {
                throw new Autd3Exception("duty must be in [0, 1)");
            }
            return FromValidated(outValue[0]);
        }
    }

    public sealed class SetPulseWidthTable : ICommand
    {
        public const int TableSize = 256;

        private readonly PulseWidth[] _table;

        public SetPulseWidthTable(PulseWidth[] table)
        {
            if (table.Length != TableSize)
            {
                throw new Autd3Exception($"pulse width table requires {TableSize} values");
            }
            _table = table;
        }

        public static PulseWidth[] DefaultTable()
        {
            var raw = new ushort[TableSize];
            NativeCommand.autd3_set_pulse_width_table_default_table(raw);
            var table = new PulseWidth[TableSize];
            for (var i = 0; i < TableSize; i++)
            {
                table[i] = PulseWidth.FromValidated(raw[i]);
            }
            return table;
        }

        IntPtr ICommand.CreateOp()
        {
            var raw = new ushort[TableSize];
            for (var i = 0; i < TableSize; i++)
            {
                raw[i] = _table[i].Value;
            }
            return NativeCommand.autd3_op_set_pulse_width_table(raw);
        }
    }

    public sealed class SetPhaseCorrection : ICommand
    {
        private readonly Phase[][] _phases;

        public SetPhaseCorrection(Phase[][] phases)
        {
            _phases = phases;
        }

        IntPtr ICommand.CreateOp()
        {
            var lens = new UIntPtr[_phases.Length];
            var total = 0;
            for (var d = 0; d < _phases.Length; d++)
            {
                lens[d] = (UIntPtr)_phases[d].Length;
                total += _phases[d].Length;
            }
            var flat = new byte[total];
            var offset = 0;
            for (var d = 0; d < _phases.Length; d++)
            {
                for (var t = 0; t < _phases[d].Length; t++)
                {
                    flat[offset + t] = _phases[d][t].Value;
                }
                offset += _phases[d].Length;
            }
            return NativeCommand.autd3_op_set_phase_correction(flat, lens, (UIntPtr)_phases.Length);
        }
    }
}
