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
        internal static extern IntPtr autd3_cpu_config_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_cpu_config_free(IntPtr config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_cpu_config(IntPtr config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_sys_time_transition_margin(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_sys_time_transition_margin(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_sync_guard(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_sync_guard(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_update_activate_delay(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_update_activate_delay(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_failsafe_timeout(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_failsafe_timeout(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_unlock_failsafe_timeout(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_unlock_failsafe_timeout(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_fpga_bus_wait(IntPtr config, byte cycles);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_fpga_bus_wait(IntPtr config, out byte cycles);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_sync_interval(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_sync_interval(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_tx_timestamp_timeout(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_tx_timestamp_timeout(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_delay_resp_timeout(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_delay_resp_timeout(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_holdover(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_holdover(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_step_threshold(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_step_threshold(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_lock_threshold(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_lock_threshold(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_fpga_wait_update_max_polls(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_fpga_wait_update_max_polls(IntPtr config, out uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_fpga_flash_max_polls(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_fpga_flash_max_polls(IntPtr config, out uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_kp_milli(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_kp_milli(IntPtr config, out uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_ki_milli(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_ki_milli(IntPtr config, out uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_max_freq_ppb(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_max_freq_ppb(IntPtr config, out uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_lock_samples(IntPtr config, ushort value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_lock_samples(IntPtr config, out ushort value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_delay_req_syncs(IntPtr config, ushort value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_delay_req_syncs(IntPtr config, out ushort value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_path_delay_filter_shift(IntPtr config, byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_path_delay_filter_shift(IntPtr config, out byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_pause_quanta(IntPtr config, ushort quanta);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_pause_quanta(IntPtr config, out ushort quanta);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_pause_hold_syncs(IntPtr config, ushort value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_pause_hold_syncs(IntPtr config, out ushort value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_set_ptp_pause_retry(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_cpu_config_get_ptp_pause_retry(IntPtr config, out ulong ns);

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
        internal static extern IntPtr autd3_op_set_phase_correction(byte[] phases, UIntPtr[] lens, UIntPtr numDevices);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_pulse_width_table(ushort[] table);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_set_pulse_width_table_default();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_pulse_width_from_duty(float duty, [Out] ushort[] outValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
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

        public ForceFan(bool value)
        {
            _value = value;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) => NativeCommand.autd3_op_force_fan(_value);
    }

    internal static class CpuConfigDefaults
    {
        internal static readonly TimeSpan SysTimeTransitionMargin;
        internal static readonly uint FpgaWaitUpdateMaxPolls;
        internal static readonly uint FpgaFlashMaxPolls;
        internal static readonly TimeSpan SyncGuard;
        internal static readonly TimeSpan UpdateActivateDelay;
        internal static readonly TimeSpan? FailsafeTimeout;
        internal static readonly TimeSpan? PtpUnlockFailsafeTimeout;
        internal static readonly FpgaBusWait FpgaBusWait;
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
            var handle = NativeCommand.autd3_cpu_config_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create cpu config");
            }
            try
            {
                SysTimeTransitionMargin = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_sys_time_transition_margin);
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_fpga_wait_update_max_polls(handle, out FpgaWaitUpdateMaxPolls));
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_fpga_flash_max_polls(handle, out FpgaFlashMaxPolls));
                SyncGuard = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_sync_guard);
                UpdateActivateDelay = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_update_activate_delay);
                var failsafeTimeout = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_failsafe_timeout);
                FailsafeTimeout = failsafeTimeout == TimeSpan.Zero ? null : failsafeTimeout;
                var ptpUnlockFailsafeTimeout = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_ptp_unlock_failsafe_timeout);
                PtpUnlockFailsafeTimeout = ptpUnlockFailsafeTimeout == TimeSpan.Zero ? null : ptpUnlockFailsafeTimeout;
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_fpga_bus_wait(handle, out var fpgaBusWait));
                FpgaBusWait = (FpgaBusWait)fpgaBusWait;
                PtpSyncInterval = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_ptp_sync_interval);
                PtpTxTimestampTimeout = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_ptp_tx_timestamp_timeout);
                PtpDelayRespTimeout = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_ptp_delay_resp_timeout);
                PtpHoldover = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_ptp_holdover);
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_ptp_lock_samples(handle, out PtpLockSamples));
                PtpStepThreshold = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_ptp_step_threshold);
                PtpLockThreshold = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_ptp_lock_threshold);
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_ptp_kp_milli(handle, out PtpKpMilli));
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_ptp_ki_milli(handle, out PtpKiMilli));
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_ptp_max_freq_ppb(handle, out PtpMaxFreqPpb));
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_ptp_delay_req_syncs(handle, out PtpDelayReqSyncs));
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_ptp_path_delay_filter_shift(handle, out PtpPathDelayFilterShift));
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_ptp_pause_quanta(handle, out var pauseQuanta));
                PtpPauseQuanta = pauseQuanta == 0 ? null : pauseQuanta;
                OptionNative.Apply("preset", NativeCommand.autd3_cpu_config_get_ptp_pause_hold_syncs(handle, out PtpPauseHoldSyncs));
                PtpPauseRetry = OptionNative.GetDuration(handle, NativeCommand.autd3_cpu_config_get_ptp_pause_retry);
            }
            finally
            {
                NativeCommand.autd3_cpu_config_free(handle);
            }
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

        internal void Apply(IntPtr handle)
        {
            OptionNative.SetRequiredDuration(handle, "ptp.syncInterval", SyncInterval, NativeCommand.autd3_cpu_config_set_ptp_sync_interval);
            OptionNative.SetRequiredDuration(handle, "ptp.txTimestampTimeout", TxTimestampTimeout, NativeCommand.autd3_cpu_config_set_ptp_tx_timestamp_timeout);
            OptionNative.SetRequiredDuration(handle, "ptp.delayRespTimeout", DelayRespTimeout, NativeCommand.autd3_cpu_config_set_ptp_delay_resp_timeout);
            OptionNative.SetRequiredDuration(handle, "ptp.holdover", Holdover, NativeCommand.autd3_cpu_config_set_ptp_holdover);
            OptionNative.Apply("ptp.lockSamples", NativeCommand.autd3_cpu_config_set_ptp_lock_samples(handle, LockSamples));
            OptionNative.SetRequiredDuration(handle, "ptp.stepThreshold", StepThreshold, NativeCommand.autd3_cpu_config_set_ptp_step_threshold);
            OptionNative.SetRequiredDuration(handle, "ptp.lockThreshold", LockThreshold, NativeCommand.autd3_cpu_config_set_ptp_lock_threshold);
            OptionNative.Apply("ptp.kpMilli", NativeCommand.autd3_cpu_config_set_ptp_kp_milli(handle, KpMilli));
            OptionNative.Apply("ptp.kiMilli", NativeCommand.autd3_cpu_config_set_ptp_ki_milli(handle, KiMilli));
            OptionNative.Apply("ptp.maxFreqPpb", NativeCommand.autd3_cpu_config_set_ptp_max_freq_ppb(handle, MaxFreqPpb));
            OptionNative.Apply("ptp.delayReqSyncs", NativeCommand.autd3_cpu_config_set_ptp_delay_req_syncs(handle, DelayReqSyncs));
            OptionNative.Apply("ptp.pathDelayFilterShift", NativeCommand.autd3_cpu_config_set_ptp_path_delay_filter_shift(handle, PathDelayFilterShift));
            OptionNative.Apply("ptp.pauseQuanta", PauseQuanta switch
            {
                null => NativeCommand.autd3_cpu_config_set_ptp_pause_quanta(handle, 0),
                { } quanta when quanta > 0 => NativeCommand.autd3_cpu_config_set_ptp_pause_quanta(handle, quanta),
                _ => -1,
            });
            OptionNative.Apply("ptp.pauseHoldSyncs", NativeCommand.autd3_cpu_config_set_ptp_pause_hold_syncs(handle, PauseHoldSyncs));
            OptionNative.SetRequiredDuration(handle, "ptp.pauseRetry", PauseRetry, NativeCommand.autd3_cpu_config_set_ptp_pause_retry);
        }
    }

    public readonly struct CpuConfig
    {
        private readonly bool _failsafeTimeoutSet;
        private readonly TimeSpan? _failsafeTimeout;
        private readonly bool _ptpUnlockFailsafeTimeoutSet;
        private readonly TimeSpan? _ptpUnlockFailsafeTimeout;

        private readonly TimeSpan? _sysTimeTransitionMargin;
        public TimeSpan SysTimeTransitionMargin { get => _sysTimeTransitionMargin ?? CpuConfigDefaults.SysTimeTransitionMargin; init => _sysTimeTransitionMargin = value; }
        private readonly uint? _fpgaWaitUpdateMaxPolls;
        public uint FpgaWaitUpdateMaxPolls { get => _fpgaWaitUpdateMaxPolls ?? CpuConfigDefaults.FpgaWaitUpdateMaxPolls; init => _fpgaWaitUpdateMaxPolls = value; }
        private readonly uint? _fpgaFlashMaxPolls;
        public uint FpgaFlashMaxPolls { get => _fpgaFlashMaxPolls ?? CpuConfigDefaults.FpgaFlashMaxPolls; init => _fpgaFlashMaxPolls = value; }
        private readonly TimeSpan? _syncGuard;
        public TimeSpan SyncGuard { get => _syncGuard ?? CpuConfigDefaults.SyncGuard; init => _syncGuard = value; }
        private readonly TimeSpan? _updateActivateDelay;
        public TimeSpan UpdateActivateDelay { get => _updateActivateDelay ?? CpuConfigDefaults.UpdateActivateDelay; init => _updateActivateDelay = value; }

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

        public PtpConfig Ptp { get; init; }

        internal IntPtr CreateHandle()
        {
            var handle = NativeCommand.autd3_cpu_config_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create cpu config");
            }
            try
            {
                OptionNative.SetRequiredDuration(handle, "sysTimeTransitionMargin", SysTimeTransitionMargin, NativeCommand.autd3_cpu_config_set_sys_time_transition_margin);
                OptionNative.Apply("fpgaWaitUpdateMaxPolls", NativeCommand.autd3_cpu_config_set_fpga_wait_update_max_polls(handle, FpgaWaitUpdateMaxPolls));
                OptionNative.Apply("fpgaFlashMaxPolls", NativeCommand.autd3_cpu_config_set_fpga_flash_max_polls(handle, FpgaFlashMaxPolls));
                OptionNative.SetRequiredDuration(handle, "syncGuard", SyncGuard, NativeCommand.autd3_cpu_config_set_sync_guard);
                OptionNative.SetRequiredDuration(handle, "updateActivateDelay", UpdateActivateDelay, NativeCommand.autd3_cpu_config_set_update_activate_delay);
                OptionNative.Apply("failsafeTimeout", FailsafeTimeout switch
                {
                    null => NativeCommand.autd3_cpu_config_set_failsafe_timeout(handle, 0),
                    { } timeout when timeout > TimeSpan.Zero => NativeCommand.autd3_cpu_config_set_failsafe_timeout(handle, OptionNative.ToNanos(timeout)),
                    _ => -1,
                });
                OptionNative.Apply("ptpUnlockFailsafeTimeout", PtpUnlockFailsafeTimeout switch
                {
                    null => NativeCommand.autd3_cpu_config_set_ptp_unlock_failsafe_timeout(handle, 0),
                    { } timeout when timeout > TimeSpan.Zero => NativeCommand.autd3_cpu_config_set_ptp_unlock_failsafe_timeout(handle, OptionNative.ToNanos(timeout)),
                    _ => -1,
                });
                OptionNative.Apply("fpgaBusWait", NativeCommand.autd3_cpu_config_set_fpga_bus_wait(handle, (byte)FpgaBusWait));
                Ptp.Apply(handle);
            }
            catch
            {
                NativeCommand.autd3_cpu_config_free(handle);
                throw;
            }
            return handle;
        }
    }

    public sealed class SetCpuConfig : ICommand
    {
        private readonly CpuConfig _config;

        public SetCpuConfig(CpuConfig config)
        {
            _config = config;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var handle = _config.CreateHandle();
            try
            {
                var op = NativeCommand.autd3_op_set_cpu_config(handle);
                if (op == IntPtr.Zero)
                {
                    throw new Autd3Exception("failed to create the cpu config operation");
                }
                return op;
            }
            finally
            {
                NativeCommand.autd3_cpu_config_free(handle);
            }
        }
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

        IntPtr ISilencerConfig.CreateOp() =>
            NativeCommand.autd3_op_set_silencer_completion_time(
                OptionNative.ToNanos(Intensity), OptionNative.ToNanos(Phase), StrictMode);
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

        public SetGpioOut(GpioOut[] outputs)
        {
            if (outputs.Length != 4)
            {
                throw new Autd3Exception("SetGpioOut requires exactly 4 outputs");
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

        public EmulateGpioIn(bool[] values)
        {
            if (values.Length != 4)
            {
                throw new Autd3Exception("EmulateGpioIn requires exactly 4 values");
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
                throw new Autd3Exception($"pulse width table requires {TableSize} values");
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

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var flat = NativeCommand.FlattenPerDevice(_phases, p => p.Value, out var lens);
            return NativeCommand.autd3_op_set_phase_correction(flat, lens, (UIntPtr)_phases.Length);
        }
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

        public static ICommand Sequence(params ICommand[] commands)
        {
            if (commands == null)
            {
                throw new ArgumentNullException(nameof(commands));
            }
            foreach (var command in commands)
            {
                if (command == null)
                {
                    throw new ArgumentNullException(nameof(commands));
                }
            }
            return new SequenceCommand((ICommand[])commands.Clone());
        }

        internal static IntPtr Compose(IntPtr[] ops, Func<IntPtr[], UIntPtr, IntPtr> compose, Action<IntPtr[]> fill)
        {
            try
            {
                fill(ops);
                var handle = compose(ops, (UIntPtr)ops.Length);
                if (handle == IntPtr.Zero)
                {
                    throw new Autd3Exception("failed to compose the commands");
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

        internal static IntPtr CreateOp(ICommand command, Geometry geometry)
        {
            var op = command.CreateOp(geometry);
            if (op == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create the command");
            }
            return op;
        }
    }

    internal sealed class EachCommand : ICommand
    {
        private readonly Func<Device, ICommand?> _factory;

        internal EachCommand(Func<Device, ICommand?> factory)
        {
            _factory = factory;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            Command.Compose(new IntPtr[geometry.NumDevices], NativeClient.autd3_command_each, ops =>
            {
                for (var i = 0; i < ops.Length; i++)
                {
                    var command = _factory(geometry[i]);
                    ops[i] = command == null ? IntPtr.Zero : Command.CreateOp(command, geometry);
                }
            });
    }

    internal sealed class SequenceCommand : ICommand
    {
        private readonly ICommand[] _commands;

        internal SequenceCommand(ICommand[] commands)
        {
            _commands = commands;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            Command.Compose(new IntPtr[_commands.Length], NativeClient.autd3_command_sequence, ops =>
            {
                for (var i = 0; i < ops.Length; i++)
                {
                    ops[i] = Command.CreateOp(_commands[i], geometry);
                }
            });
    }
}
