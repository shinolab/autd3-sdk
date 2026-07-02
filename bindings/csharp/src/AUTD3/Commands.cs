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
        internal static extern IntPtr autd3_op_set_output_mask(byte[] masks, UIntPtr numDevices);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_set_phase_correction(byte[] phases, UIntPtr numDevices);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_set_pulse_width_table(ushort[] table);

        [DllImport(Lib)]
        internal static extern void autd3_pulse_width_default_table([Out] ushort[] outTable);

        [DllImport(Lib)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_pulse_width_from_duty(float duty, [Out] ushort[] outValue);
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
        public static GpioOut SysTimeEq(ulong sysTime) => new GpioOut(10, sysTime);
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

    public sealed class SetSilencer : ICommand
    {
        private readonly Func<IntPtr> _create;

        private SetSilencer(Func<IntPtr> create)
        {
            _create = create;
        }

        public static SetSilencer FromCompletionTime(TimeSpan intensity, TimeSpan phase, bool strict = true) =>
            new SetSilencer(() => NativeCommand.autd3_op_set_silencer_completion_time(
                (ulong)(intensity.Ticks * 100), (ulong)(phase.Ticks * 100), strict));

        public static SetSilencer FromUpdateRate(ushort intensity, ushort phase) =>
            new SetSilencer(() =>
            {
                var op = NativeCommand.autd3_op_set_silencer_update_rate(intensity, phase);
                if (op == IntPtr.Zero)
                {
                    throw new Autd3Exception("silencer update rate must be >= 1");
                }
                return op;
            });

        public static SetSilencer Disable() =>
            new SetSilencer(NativeCommand.autd3_op_set_silencer_disable);

        IntPtr ICommand.CreateOp() => _create();
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
        private const int NumTransducers = 249;

        private readonly bool[][] _masks;

        public SetOutputMask(bool[][] masks)
        {
            _masks = masks;
        }

        IntPtr ICommand.CreateOp()
        {
            var flat = new byte[_masks.Length * NumTransducers];
            for (var d = 0; d < _masks.Length; d++)
            {
                if (_masks[d].Length != NumTransducers)
                {
                    throw new Autd3Exception($"each device mask requires {NumTransducers} values");
                }
                for (var t = 0; t < NumTransducers; t++)
                {
                    flat[d * NumTransducers + t] = (byte)(_masks[d][t] ? 1 : 0);
                }
            }
            return NativeCommand.autd3_op_set_output_mask(flat, (UIntPtr)_masks.Length);
        }
    }

    public static class PulseWidth
    {
        public const int TableSize = 256;

        public static ushort[] DefaultTable()
        {
            var table = new ushort[TableSize];
            NativeCommand.autd3_pulse_width_default_table(table);
            return table;
        }

        public static ushort FromDuty(float duty)
        {
            var outValue = new ushort[1];
            if (!NativeCommand.autd3_pulse_width_from_duty(duty, outValue))
            {
                throw new Autd3Exception("duty must be in [0, 1)");
            }
            return outValue[0];
        }
    }

    public sealed class SetPulseWidthTable : ICommand
    {
        private readonly ushort[] _table;

        public SetPulseWidthTable(ushort[] table)
        {
            if (table.Length != PulseWidth.TableSize)
            {
                throw new Autd3Exception($"pulse width table requires {PulseWidth.TableSize} values");
            }
            _table = table;
        }

        IntPtr ICommand.CreateOp() => NativeCommand.autd3_op_set_pulse_width_table(_table);
    }

    public sealed class SetPhaseCorrection : ICommand
    {
        private const int NumTransducers = 249;

        private readonly Phase[][] _phases;

        public SetPhaseCorrection(Phase[][] phases)
        {
            _phases = phases;
        }

        IntPtr ICommand.CreateOp()
        {
            var flat = new byte[_phases.Length * NumTransducers];
            for (var d = 0; d < _phases.Length; d++)
            {
                if (_phases[d].Length != NumTransducers)
                {
                    throw new Autd3Exception($"each device phase correction requires {NumTransducers} values");
                }
                for (var t = 0; t < NumTransducers; t++)
                {
                    flat[d * NumTransducers + t] = _phases[d][t].Value;
                }
            }
            return NativeCommand.autd3_op_set_phase_correction(flat, (UIntPtr)_phases.Length);
        }
    }
}
