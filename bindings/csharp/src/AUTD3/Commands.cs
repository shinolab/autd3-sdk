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

    [StructLayout(LayoutKind.Sequential)]
    internal struct PtpConfigNative
    {
        public ulong SyncIntervalNs;
        public ulong TxTimestampTimeoutNs;
        public ulong DelayRespTimeoutNs;
        public ulong HoldoverNs;
        public ushort LockSamples;
        public ulong StepThresholdNs;
        public ulong LockThresholdNs;
        public uint KpMilli;
        public uint KiMilli;
        public uint MaxFreqPpb;
        public ushort DelayReqSyncs;
        public byte PathDelayFilterShift;
        public ushort PauseQuanta;
        public ushort PauseHoldSyncs;
        public ulong PauseRetryNs;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct CpuConfigNative
    {
        public ulong SyncGuardNs;
        public byte HasFailsafeTimeout;
        public ulong FailsafeTimeoutNs;
        public byte HasPtpUnlockFailsafeTimeout;
        public ulong PtpUnlockFailsafeTimeoutNs;
        public byte FpgaBusWait;
        public byte HasCpuWatchdogTimeout;
        public ulong CpuWatchdogTimeoutNs;
        public PtpConfigNative Ptp;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct FpgaConfigNative
    {
        public byte SyncPhaseLock;
    }

    internal static class NativeCommand
    {
        private const string Lib = "autd3capi";

        static NativeCommand() => NativeAbi.Verify(Lib, autd3_abi_version());

        internal static byte[] FlattenPerDevice<T>(T[][] src, Func<T, byte> value, out UIntPtr[] lens)
        {
            lens = new UIntPtr[src.Length];
            var total = 0;
            for (var d = 0; d < src.Length; d++)
            {
                lens[d] = (UIntPtr)src[d].Length;
                total += src[d].Length;
            }
            var flat = new byte[total];
            var offset = 0;
            for (var d = 0; d < src.Length; d++)
            {
                for (var t = 0; t < src[d].Length; t++)
                {
                    flat[offset + t] = value(src[d][t]);
                }
                offset += src[d].Length;
            }
            return flat;
        }

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_clear();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_synchronize();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_release_failsafe();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_nop();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_force_fan([MarshalAs(UnmanagedType.I1)] bool value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_default(out CpuConfigNative config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_cpu_config(CpuConfigNative config, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_fpga_config_default(out FpgaConfigNative config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_fpga_config(FpgaConfigNative config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_silencer_completion_time(ulong intensityNs, ulong phaseNs, [MarshalAs(UnmanagedType.I1)] bool strict);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_silencer_update_rate(ushort intensity, ushort phase);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_silencer_disable();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_gpio_out(GpioOutNative[] outputs);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_emulate_gpio_in(byte[] values);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_output_mask(byte[] masks, UIntPtr[] lens, UIntPtr numDevices);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_phase_correction(PhaseBufferHandle phases);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_pulse_width_table(ushort[] table);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_pulse_width_table_default();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pulse_width_new(ushort raw, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pulse_width_from_duty(float duty, out ushort @out, byte[] outErr, UIntPtr outErrLen);
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
        public static GpioOut SysTimeEq(SysTime sysTime) => new GpioOut(10, sysTime.Nanos);
        public static GpioOut SyncDiff => new GpioOut(11, 0);
        public static GpioOut PwmOut(byte transducer) => new GpioOut(12, transducer);
        public static GpioOut Direct(bool on) => new GpioOut(13, on ? 1UL : 0UL);

        internal GpioOutNative ToNative() => new GpioOutNative { Kind = Kind, Value = Value };
    }

    public sealed class Clear : ICommand
    {
        IntPtr ICommand.CreateOp(Geometry geometry) => NativeCommand.autd3_op_clear();
    }

    public sealed class Synchronize : ICommand
    {
        IntPtr ICommand.CreateOp(Geometry geometry) => NativeCommand.autd3_op_synchronize();
    }

    public sealed class ReleaseFailsafe : ICommand
    {
        IntPtr ICommand.CreateOp(Geometry geometry) => NativeCommand.autd3_op_release_failsafe();
    }

    public sealed class Nop : ICommand
    {
        IntPtr ICommand.CreateOp(Geometry geometry) => NativeCommand.autd3_op_nop();
    }

    public sealed class ForceFan : ICommand
    {
        private readonly bool _value;

        public ForceFan()
        {
            _value = false;
        }

        public ForceFan(bool value)
        {
            _value = value;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) => NativeCommand.autd3_op_force_fan(_value);
    }

    internal static class CpuConfigDefaults
    {
        internal static readonly TimeSpan SyncGuard;
        internal static readonly TimeSpan? FailsafeTimeout;
        internal static readonly TimeSpan? PtpUnlockFailsafeTimeout;
        internal static readonly FpgaBusWait FpgaBusWait;
        internal static readonly TimeSpan? CpuWatchdogTimeout;
        internal static readonly TimeSpan PtpSyncInterval;
        internal static readonly TimeSpan PtpTxTimestampTimeout;
        internal static readonly TimeSpan PtpDelayRespTimeout;
        internal static readonly TimeSpan PtpHoldover;
        internal static readonly ushort PtpLockSamples;
        internal static readonly TimeSpan PtpStepThreshold;
        internal static readonly TimeSpan PtpLockThreshold;
        internal static readonly uint PtpKpMilli;
        internal static readonly uint PtpKiMilli;
        internal static readonly uint PtpMaxFreqPpb;
        internal static readonly ushort PtpDelayReqSyncs;
        internal static readonly byte PtpPathDelayFilterShift;
        internal static readonly ushort? PtpPauseQuanta;
        internal static readonly ushort PtpPauseHoldSyncs;
        internal static readonly TimeSpan PtpPauseRetry;

        static CpuConfigDefaults()
        {
            OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_default(out var native));
            SyncGuard = OptionNative.FromNanos(native.SyncGuardNs);
            FailsafeTimeout = OptionNative.FromOptionalNanos(native.HasFailsafeTimeout, native.FailsafeTimeoutNs);
            PtpUnlockFailsafeTimeout = OptionNative.FromOptionalNanos(native.HasPtpUnlockFailsafeTimeout, native.PtpUnlockFailsafeTimeoutNs);
            FpgaBusWait = FpgaBusWaitNative.FromCycles(native.FpgaBusWait);
            CpuWatchdogTimeout = OptionNative.FromOptionalNanos(native.HasCpuWatchdogTimeout, native.CpuWatchdogTimeoutNs);
            var ptp = native.Ptp;
            PtpSyncInterval = OptionNative.FromNanos(ptp.SyncIntervalNs);
            PtpTxTimestampTimeout = OptionNative.FromNanos(ptp.TxTimestampTimeoutNs);
            PtpDelayRespTimeout = OptionNative.FromNanos(ptp.DelayRespTimeoutNs);
            PtpHoldover = OptionNative.FromNanos(ptp.HoldoverNs);
            PtpLockSamples = ptp.LockSamples;
            PtpStepThreshold = OptionNative.FromNanos(ptp.StepThresholdNs);
            PtpLockThreshold = OptionNative.FromNanos(ptp.LockThresholdNs);
            PtpKpMilli = ptp.KpMilli;
            PtpKiMilli = ptp.KiMilli;
            PtpMaxFreqPpb = ptp.MaxFreqPpb;
            PtpDelayReqSyncs = ptp.DelayReqSyncs;
            PtpPathDelayFilterShift = ptp.PathDelayFilterShift;
            PtpPauseQuanta = ptp.PauseQuanta == 0 ? null : ptp.PauseQuanta;
            PtpPauseHoldSyncs = ptp.PauseHoldSyncs;
            PtpPauseRetry = OptionNative.FromNanos(ptp.PauseRetryNs);
        }
    }

    public readonly struct PtpConfig
    {
        private readonly TimeSpan? _syncInterval;
        public TimeSpan SyncInterval { get => _syncInterval ?? CpuConfigDefaults.PtpSyncInterval; init => _syncInterval = value; }
        private readonly TimeSpan? _txTimestampTimeout;
        public TimeSpan TxTimestampTimeout { get => _txTimestampTimeout ?? CpuConfigDefaults.PtpTxTimestampTimeout; init => _txTimestampTimeout = value; }
        private readonly TimeSpan? _delayRespTimeout;
        public TimeSpan DelayRespTimeout { get => _delayRespTimeout ?? CpuConfigDefaults.PtpDelayRespTimeout; init => _delayRespTimeout = value; }
        private readonly TimeSpan? _holdover;
        public TimeSpan Holdover { get => _holdover ?? CpuConfigDefaults.PtpHoldover; init => _holdover = value; }
        private readonly ushort? _lockSamples;
        public ushort LockSamples { get => _lockSamples ?? CpuConfigDefaults.PtpLockSamples; init => _lockSamples = value; }
        private readonly TimeSpan? _stepThreshold;
        public TimeSpan StepThreshold { get => _stepThreshold ?? CpuConfigDefaults.PtpStepThreshold; init => _stepThreshold = value; }
        private readonly TimeSpan? _lockThreshold;
        public TimeSpan LockThreshold { get => _lockThreshold ?? CpuConfigDefaults.PtpLockThreshold; init => _lockThreshold = value; }
        private readonly uint? _kpMilli;
        public uint KpMilli { get => _kpMilli ?? CpuConfigDefaults.PtpKpMilli; init => _kpMilli = value; }
        private readonly uint? _kiMilli;
        public uint KiMilli { get => _kiMilli ?? CpuConfigDefaults.PtpKiMilli; init => _kiMilli = value; }
        private readonly uint? _maxFreqPpb;
        public uint MaxFreqPpb { get => _maxFreqPpb ?? CpuConfigDefaults.PtpMaxFreqPpb; init => _maxFreqPpb = value; }
        private readonly ushort? _delayReqSyncs;
        public ushort DelayReqSyncs { get => _delayReqSyncs ?? CpuConfigDefaults.PtpDelayReqSyncs; init => _delayReqSyncs = value; }
        private readonly byte? _pathDelayFilterShift;
        public byte PathDelayFilterShift { get => _pathDelayFilterShift ?? CpuConfigDefaults.PtpPathDelayFilterShift; init => _pathDelayFilterShift = value; }
        private readonly bool _pauseQuantaSet;
        private readonly ushort? _pauseQuanta;

        public ushort? PauseQuanta
        {
            get => _pauseQuantaSet ? _pauseQuanta : CpuConfigDefaults.PtpPauseQuanta;
            init
            {
                _pauseQuanta = value;
                _pauseQuantaSet = true;
            }
        }

        private readonly ushort? _pauseHoldSyncs;
        public ushort PauseHoldSyncs { get => _pauseHoldSyncs ?? CpuConfigDefaults.PtpPauseHoldSyncs; init => _pauseHoldSyncs = value; }
        private readonly TimeSpan? _pauseRetry;
        public TimeSpan PauseRetry { get => _pauseRetry ?? CpuConfigDefaults.PtpPauseRetry; init => _pauseRetry = value; }

        internal PtpConfigNative ToNative() => new PtpConfigNative
        {
            SyncIntervalNs = OptionNative.RequiredNanos("ptp.syncInterval", SyncInterval),
            TxTimestampTimeoutNs = OptionNative.RequiredNanos("ptp.txTimestampTimeout", TxTimestampTimeout),
            DelayRespTimeoutNs = OptionNative.RequiredNanos("ptp.delayRespTimeout", DelayRespTimeout),
            HoldoverNs = OptionNative.RequiredNanos("ptp.holdover", Holdover),
            LockSamples = LockSamples,
            StepThresholdNs = OptionNative.RequiredNanos("ptp.stepThreshold", StepThreshold),
            LockThresholdNs = OptionNative.RequiredNanos("ptp.lockThreshold", LockThreshold),
            KpMilli = KpMilli,
            KiMilli = KiMilli,
            MaxFreqPpb = MaxFreqPpb,
            DelayReqSyncs = DelayReqSyncs,
            PathDelayFilterShift = PathDelayFilterShift,
            PauseQuanta = PauseQuanta switch
            {
                null => 0,
                { } quanta when quanta > 0 => quanta,
                _ => throw OptionNative.OutOfRange("ptp.pauseQuanta"),
            },
            PauseHoldSyncs = PauseHoldSyncs,
            PauseRetryNs = OptionNative.RequiredNanos("ptp.pauseRetry", PauseRetry),
        };
    }

    public readonly struct CpuConfig
    {
        private readonly bool _failsafeTimeoutSet;
        private readonly TimeSpan? _failsafeTimeout;
        private readonly bool _ptpUnlockFailsafeTimeoutSet;
        private readonly TimeSpan? _ptpUnlockFailsafeTimeout;
        private readonly bool _cpuWatchdogTimeoutSet;
        private readonly TimeSpan? _cpuWatchdogTimeout;

        private readonly TimeSpan? _syncGuard;
        public TimeSpan SyncGuard { get => _syncGuard ?? CpuConfigDefaults.SyncGuard; init => _syncGuard = value; }

        public TimeSpan? FailsafeTimeout
        {
            get => _failsafeTimeoutSet ? _failsafeTimeout : CpuConfigDefaults.FailsafeTimeout;
            init
            {
                _failsafeTimeout = value;
                _failsafeTimeoutSet = true;
            }
        }

        public TimeSpan? PtpUnlockFailsafeTimeout
        {
            get => _ptpUnlockFailsafeTimeoutSet ? _ptpUnlockFailsafeTimeout : CpuConfigDefaults.PtpUnlockFailsafeTimeout;
            init
            {
                _ptpUnlockFailsafeTimeout = value;
                _ptpUnlockFailsafeTimeoutSet = true;
            }
        }

        private readonly FpgaBusWait? _fpgaBusWait;
        public FpgaBusWait FpgaBusWait { get => _fpgaBusWait ?? CpuConfigDefaults.FpgaBusWait; init => _fpgaBusWait = value; }

        public TimeSpan? CpuWatchdogTimeout
        {
            get => _cpuWatchdogTimeoutSet ? _cpuWatchdogTimeout : CpuConfigDefaults.CpuWatchdogTimeout;
            init
            {
                _cpuWatchdogTimeout = value;
                _cpuWatchdogTimeoutSet = true;
            }
        }

        public PtpConfig Ptp { get; init; }

        internal CpuConfigNative ToNative() => new CpuConfigNative
        {
            SyncGuardNs = OptionNative.RequiredNanos("syncGuard", SyncGuard),
            HasFailsafeTimeout = OptionNative.Flag(FailsafeTimeout.HasValue),
            FailsafeTimeoutNs = OptionNative.OptionalNanos("failsafeTimeout", FailsafeTimeout),
            HasPtpUnlockFailsafeTimeout = OptionNative.Flag(PtpUnlockFailsafeTimeout.HasValue),
            PtpUnlockFailsafeTimeoutNs = OptionNative.OptionalNanos("ptpUnlockFailsafeTimeout", PtpUnlockFailsafeTimeout),
            FpgaBusWait = FpgaBusWaitNative.ToCycles(FpgaBusWait),
            HasCpuWatchdogTimeout = OptionNative.Flag(CpuWatchdogTimeout.HasValue),
            CpuWatchdogTimeoutNs = OptionNative.OptionalNanos("cpuWatchdogTimeout", CpuWatchdogTimeout),
            Ptp = Ptp.ToNative(),
        };
    }

    public sealed class SetCpuConfig : ICommand
    {
        private readonly CpuConfig _config;

        public SetCpuConfig() : this(new CpuConfig())
        {
        }

        public SetCpuConfig(CpuConfig config)
        {
            _config = config;
        }

        public CpuConfig Config => _config;

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var native = _config.ToNative();
            var err = new byte[NativeAbi.ErrorBufferLength];
            var op = NativeCommand.autd3_op_set_cpu_config(native, err, (UIntPtr)err.Length);
            if (op == IntPtr.Zero)
            {
                throw Autd3Exception.FromNative((int)Autd3ErrorCode.InvalidArgument, err);
            }
            return op;
        }
    }

    internal static class FpgaConfigDefaults
    {
        internal static readonly bool SyncPhaseLock;

        static FpgaConfigDefaults()
        {
            OptionNative.Apply("preset", NativeCommand.autd3_fpga_config_default(out var native));
            SyncPhaseLock = native.SyncPhaseLock != 0;
        }
    }

    public readonly struct FpgaConfig
    {
        private readonly bool? _syncPhaseLock;
        public bool SyncPhaseLock { get => _syncPhaseLock ?? FpgaConfigDefaults.SyncPhaseLock; init => _syncPhaseLock = value; }

        internal FpgaConfigNative ToNative() => new FpgaConfigNative
        {
            SyncPhaseLock = OptionNative.Flag(SyncPhaseLock),
        };
    }

    public sealed class SetFpgaConfig : ICommand
    {
        private readonly FpgaConfig _config;

        public SetFpgaConfig() : this(new FpgaConfig())
        {
        }

        public SetFpgaConfig(FpgaConfig config)
        {
            _config = config;
        }

        public FpgaConfig Config => _config;

        IntPtr ICommand.CreateOp(Geometry geometry) => NativeCommand.autd3_op_set_fpga_config(_config.ToNative());
    }

    public interface ISilencerConfig
    {
        internal IntPtr CreateOp();
    }

    internal static class SilencerDefaults
    {
        internal static readonly TimeSpan CompletionIntensity;
        internal static readonly TimeSpan CompletionPhase;
        internal static readonly bool StrictMode;

        static SilencerDefaults()
        {
            OptionNative.Apply("preset", NativeClient.autd3_silencer_default_completion_time(out var intensity, out var phase, out StrictMode));
            CompletionIntensity = OptionNative.FromNanos(intensity);
            CompletionPhase = OptionNative.FromNanos(phase);
        }
    }

    public readonly struct FixedCompletionTime : ISilencerConfig
    {
        private readonly TimeSpan? _intensity;
        public TimeSpan Intensity { get => _intensity ?? SilencerDefaults.CompletionIntensity; init => _intensity = value; }
        private readonly TimeSpan? _phase;
        public TimeSpan Phase { get => _phase ?? SilencerDefaults.CompletionPhase; init => _phase = value; }
        private readonly bool? _strictMode;
        public bool StrictMode { get => _strictMode ?? SilencerDefaults.StrictMode; init => _strictMode = value; }

        IntPtr ISilencerConfig.CreateOp()
        {
            OptionNative.Apply("intensity", Intensity < TimeSpan.Zero ? -1 : 0);
            OptionNative.Apply("phase", Phase < TimeSpan.Zero ? -1 : 0);
            return NativeCommand.autd3_op_set_silencer_completion_time(
                OptionNative.ToNanos(Intensity), OptionNative.ToNanos(Phase), StrictMode);
        }
    }

    public readonly struct FixedUpdateRate : ISilencerConfig
    {
        public ushort Intensity { get; }
        public ushort Phase { get; }

        public FixedUpdateRate(ushort intensity, ushort phase)
        {
            Intensity = intensity;
            Phase = phase;
        }

        IntPtr ISilencerConfig.CreateOp()
        {
            var op = NativeCommand.autd3_op_set_silencer_update_rate(Intensity, Phase);
            if (op == IntPtr.Zero)
            {
                throw new Autd3Exception("silencer update rate must be >= 1", Autd3ErrorCode.InvalidArgument);
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

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            _disable ? NativeCommand.autd3_op_set_silencer_disable() : _config!.CreateOp();
    }

    public sealed class SetGpioOut : ICommand
    {
        private readonly GpioOut[] _outputs;

        public SetGpioOut() : this(new[] { GpioOut.Off, GpioOut.Off, GpioOut.Off, GpioOut.Off })
        {
        }

        public SetGpioOut(GpioOut[] outputs)
        {
            if (outputs.Length != 4)
            {
                throw new Autd3Exception("SetGpioOut requires exactly 4 outputs", Autd3ErrorCode.InvalidArgument);
            }
            _outputs = outputs;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
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

        public EmulateGpioIn() : this(new bool[4])
        {
        }

        public EmulateGpioIn(bool[] values)
        {
            if (values.Length != 4)
            {
                throw new Autd3Exception("EmulateGpioIn requires exactly 4 values", Autd3ErrorCode.InvalidArgument);
            }
            _values = values;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
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

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var flat = NativeCommand.FlattenPerDevice(_masks, m => (byte)(m ? 1 : 0), out var lens);
            return NativeCommand.autd3_op_set_output_mask(flat, lens, (UIntPtr)_masks.Length);
        }
    }

    public readonly struct PulseWidth : IEquatable<PulseWidth>
    {
        private readonly ushort _value;

        public PulseWidth(ushort pulseWidth)
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            if (NativeCommand.autd3_pulse_width_new(pulseWidth, err, (UIntPtr)err.Length) != 0)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err), Autd3ErrorCode.InvalidArgument);
            }
            _value = pulseWidth;
        }

        public static PulseWidth FromDuty(float duty)
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            if (NativeCommand.autd3_pulse_width_from_duty(duty, out var value, err, (UIntPtr)err.Length) != 0)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err), Autd3ErrorCode.InvalidArgument);
            }
            return new PulseWidth(value);
        }

        public ushort Value => _value;

        public bool Equals(PulseWidth other) => _value == other._value;

        public override bool Equals(object? obj) => obj is PulseWidth other && Equals(other);

        public override int GetHashCode() => _value;

        public static bool operator ==(PulseWidth left, PulseWidth right) => left.Equals(right);

        public static bool operator !=(PulseWidth left, PulseWidth right) => !left.Equals(right);
    }

    public sealed class SetPulseWidthTable : ICommand
    {
        public static readonly int TableSize = (int)NativeClient.autd3_params_pwe_table_size();

        private readonly PulseWidth[]? _table;

        public SetPulseWidthTable()
        {
            _table = null;
        }

        public SetPulseWidthTable(PulseWidth[] table)
        {
            if (table.Length != TableSize)
            {
                throw new Autd3Exception($"pulse width table requires {TableSize} values", Autd3ErrorCode.InvalidArgument);
            }
            _table = table;
        }

        public static PulseWidth[] EmptyTable() => new PulseWidth[TableSize];

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            if (_table is null)
            {
                return NativeCommand.autd3_op_set_pulse_width_table_default();
            }
            var native = new ushort[TableSize];
            for (var i = 0; i < TableSize; i++)
            {
                native[i] = _table[i].Value;
            }
            return NativeCommand.autd3_op_set_pulse_width_table(native);
        }
    }

    public sealed class SetPhaseCorrection : ICommand
    {
        private readonly PhaseBuffer _phases;

        public SetPhaseCorrection(PhaseBuffer phases)
        {
            _phases = phases;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) => NativeCommand.autd3_op_set_phase_correction(_phases.Handle);
    }

    public static class Command
    {
        public static ICommand Each(Func<Device, ICommand?> factory)
        {
            if (factory == null)
            {
                throw new ArgumentNullException(nameof(factory));
            }
            return new EachCommand(factory);
        }
    }

    internal sealed class EachCommand : ICommand
    {
        private readonly Func<Device, ICommand?> _factory;

        internal EachCommand(Func<Device, ICommand?> factory)
        {
            _factory = factory;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var ops = new IntPtr[geometry.NumDevices];
            try
            {
                for (var i = 0; i < ops.Length; i++)
                {
                    var command = _factory(geometry[i]);
                    if (command == null)
                    {
                        continue;
                    }
                    ops[i] = command.CreateOp(geometry);
                    if (ops[i] == IntPtr.Zero)
                    {
                        throw new Autd3Exception("failed to create the command", Autd3ErrorCode.InvalidArgument);
                    }
                }
                var handle = NativeClient.autd3_command_each(ops, (UIntPtr)ops.Length);
                if (handle == IntPtr.Zero)
                {
                    throw new Autd3Exception("failed to compose the commands", Autd3ErrorCode.InvalidArgument);
                }
                return handle;
            }
            catch
            {
                foreach (var op in ops)
                {
                    if (op != IntPtr.Zero)
                    {
                        NativeClient.autd3_op_free(op);
                    }
                }
                throw;
            }
        }
    }

}
