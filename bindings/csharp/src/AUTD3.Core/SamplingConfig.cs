using System;

namespace AUTD3
{
    public readonly struct SamplingConfig : IEquatable<SamplingConfig>
    {
        private enum Kind : byte
        {
            Unset,
            Divide,
            Freq,
            FreqNearest,
            Period,
            PeriodNearest,
        }

        private readonly Kind _kind;
        private readonly ushort _divide;
        private readonly float _freq;
        private readonly ulong _periodNs;
        private readonly ushort _resolved;

        private SamplingConfig(Kind kind, ushort divide, float freq, ulong periodNs)
        {
            _kind = kind;
            _divide = divide;
            _freq = freq;
            _periodNs = periodNs;
            _resolved = 0;
            _resolved = Resolve(out _);
        }

        public SamplingConfig(ushort divide)
        {
            if (divide == 0)
            {
                throw new Autd3Exception("sampling divide must be >= 1", Autd3ErrorCode.InvalidArgument);
            }
            _kind = Kind.Divide;
            _divide = divide;
            _freq = 0f;
            _periodNs = 0;
            _resolved = divide;
        }

        public SamplingConfig(Freq freq) : this(Kind.Freq, 0, freq.Hz, 0)
        {
        }

        public SamplingConfig(TimeSpan period) : this(Kind.Period, 0, 0f, PeriodNanos(period))
        {
        }

        public SamplingConfig(Nearest<Freq> freq) : this(Kind.FreqNearest, 0, freq.Value.Hz, 0)
        {
        }

        public SamplingConfig(Nearest<TimeSpan> period) : this(Kind.PeriodNearest, 0, 0f, PeriodNanos(period.Value))
        {
        }

        private static ulong PeriodNanos(TimeSpan period)
        {
            if (period < TimeSpan.Zero)
            {
                throw new Autd3Exception("a sampling period must not be negative", Autd3ErrorCode.InvalidArgument);
            }
            return OptionNative.ToNanos(period);
        }

        public static SamplingConfig Freq4k => new SamplingConfig(4000 * Units.Hz);

        public static SamplingConfig Freq40k => new SamplingConfig(40000 * Units.Hz);

        public ushort Divide()
        {
            if (_resolved != 0)
            {
                return _resolved;
            }
            var divide = Resolve(out var error);
            if (divide == 0)
            {
                throw new Autd3Exception(error, Autd3ErrorCode.InvalidArgument);
            }
            return divide;
        }

        public Freq Freq() => (float)Params.UltrasoundFreqHz / Divide() * Units.Hz;

        public TimeSpan Period() => TimeSpan.FromTicks(Params.UltrasoundPeriod.Ticks * Divide());

        public bool Equals(SamplingConfig other)
        {
            var lhs = TryDivide();
            return lhs != 0 && lhs == other.TryDivide();
        }

        public override bool Equals(object? obj) => obj is SamplingConfig other && Equals(other);

        public override int GetHashCode() => TryDivide();

        public static bool operator ==(SamplingConfig left, SamplingConfig right) => left.Equals(right);

        public static bool operator !=(SamplingConfig left, SamplingConfig right) => !left.Equals(right);

        private ushort TryDivide() => _resolved != 0 ? _resolved : Resolve(out _);

        private const string UnsetMessage = "the sampling config is not set (default(SamplingConfig) has no value)";

        private ushort Resolve(out string error)
        {
            error = string.Empty;
            if (_kind == Kind.Unset)
            {
                error = UnsetMessage;
                return 0;
            }
            var handle = CreateHandle();
            try
            {
                var err = new byte[NativeAbi.ErrorBufferLength];
                if (NativeCore.autd3_core_sampling_config_resolve(handle, out var value, err, (UIntPtr)err.Length) != 0)
                {
                    error = NativeUtil.Utf8(err);
                    return 0;
                }
                return value;
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(handle);
            }
        }

        internal IntPtr CreateHandle()
        {
            if (_kind == Kind.Unset)
            {
                throw new Autd3Exception(UnsetMessage, Autd3ErrorCode.InvalidArgument);
            }
            var handle = _kind switch
            {
                Kind.Divide => NativeCore.autd3_core_sampling_config_divide(_divide),
                Kind.Freq => NativeCore.autd3_core_sampling_config_freq(_freq),
                Kind.FreqNearest => NativeCore.autd3_core_sampling_config_freq_nearest(_freq),
                Kind.Period => NativeCore.autd3_core_sampling_config_period(_periodNs),
                _ => NativeCore.autd3_core_sampling_config_period_nearest(_periodNs),
            };
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create sampling config");
            }
            return handle;
        }
    }
}
