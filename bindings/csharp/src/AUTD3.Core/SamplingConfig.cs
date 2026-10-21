using System;

namespace AUTD3
{
    public readonly struct SamplingConfig : IEquatable<SamplingConfig>
    {
        private readonly ushort _divide;

        private const string UnsetMessage = "the sampling config is not set (default(SamplingConfig) has no value)";

        public SamplingConfig(ushort divide)
        {
            if (divide == 0)
            {
                throw new Autd3Exception("sampling divide must be >= 1", Autd3ErrorCode.InvalidArgument);
            }
            _divide = divide;
        }

        public SamplingConfig(Freq freq)
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            _divide = Checked(NativeCore.autd3_core_sampling_config_from_freq(freq.Hz, out var divide, err, (UIntPtr)err.Length), divide, err);
        }

        public SamplingConfig(TimeSpan period)
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            _divide = Checked(NativeCore.autd3_core_sampling_config_from_period(PeriodNanos(period), out var divide, err, (UIntPtr)err.Length), divide, err);
        }

        public SamplingConfig(Nearest<Freq> freq)
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            _divide = Checked(NativeCore.autd3_core_sampling_config_from_freq_nearest(freq.Value.Hz, out var divide, err, (UIntPtr)err.Length), divide, err);
        }

        public SamplingConfig(Nearest<TimeSpan> period)
        {
            _divide = NativeCore.autd3_core_sampling_config_from_period_nearest(PeriodNanos(period.Value));
        }

        private static ushort Checked(int code, ushort divide, byte[] err)
        {
            if (code != 0)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err), Autd3ErrorCode.InvalidArgument);
            }
            return divide;
        }

        private static ulong PeriodNanos(TimeSpan period)
        {
            if (period < TimeSpan.Zero)
            {
                throw new Autd3Exception("a sampling period must not be negative", Autd3ErrorCode.InvalidArgument);
            }
            return OptionNative.ToNanos(period);
        }

        public static SamplingConfig Freq4k => new SamplingConfig(NativeCore.autd3_core_sampling_config_freq_4k());

        public static SamplingConfig Freq40k => new SamplingConfig(NativeCore.autd3_core_sampling_config_freq_40k());

        public ushort Divide()
        {
            if (_divide == 0)
            {
                throw new Autd3Exception(UnsetMessage, Autd3ErrorCode.InvalidArgument);
            }
            return _divide;
        }

        internal ushort DivideForNative() => Divide();

        public Freq Freq() => (float)Params.UltrasoundFreqHz / Divide() * Units.Hz;

        public TimeSpan Period() => TimeSpan.FromTicks(Params.UltrasoundPeriod.Ticks * Divide());

        public bool Equals(SamplingConfig other) => _divide == other._divide;

        public override bool Equals(object? obj) => obj is SamplingConfig other && Equals(other);

        public override int GetHashCode() => _divide;

        public static bool operator ==(SamplingConfig left, SamplingConfig right) => left.Equals(right);

        public static bool operator !=(SamplingConfig left, SamplingConfig right) => !left.Equals(right);
    }
}
