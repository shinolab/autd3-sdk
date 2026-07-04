using System;

namespace AUTD3
{


    public readonly struct SamplingConfig
    {
        private enum Kind : byte
        {
            // default(SamplingConfig) must stay usable: treat it as FREQ_4K like SineOption's default
            Default,
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

        private SamplingConfig(Kind kind, ushort divide, float freq, ulong periodNs)
        {
            _kind = kind;
            _divide = divide;
            _freq = freq;
            _periodNs = periodNs;
        }

        public SamplingConfig(ushort divide)
        {
            if (divide == 0)
            {
                throw new Autd3Exception("sampling divide must be >= 1");
            }
            _kind = Kind.Divide;
            _divide = divide;
            _freq = 0f;
            _periodNs = 0;
        }

        public SamplingConfig(Freq freq) : this(Kind.Freq, 0, freq.Hz, 0)
        {
        }

        public SamplingConfig(TimeSpan period) : this(Kind.Period, 0, 0f, (ulong)(period.Ticks * 100))
        {
        }

        public SamplingConfig(Nearest<Freq> freq) : this(Kind.FreqNearest, 0, freq.Value.Hz, 0)
        {
        }

        public SamplingConfig(Nearest<TimeSpan> period) : this(Kind.PeriodNearest, 0, 0f, (ulong)(period.Value.Ticks * 100))
        {
        }

        public static SamplingConfig Freq4k => new SamplingConfig(4000 * Units.Hz);

        public static SamplingConfig Freq40k => new SamplingConfig(40000 * Units.Hz);

        public ushort Divide()
        {
            var handle = CreateHandle();
            try
            {
                if (NativeCore.autd3_core_sampling_config_divide_value(handle, out var value) != 0)
                {
                    throw new Autd3Exception("sampling config cannot be resolved to a divider");
                }
                return value;
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(handle);
            }
        }

        public Freq Freq()
        {
            var handle = CreateHandle();
            try
            {
                if (NativeCore.autd3_core_sampling_config_freq_value(handle, out var value) != 0)
                {
                    throw new Autd3Exception("sampling config cannot be resolved to a frequency");
                }
                return value * Units.Hz;
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(handle);
            }
        }

        public TimeSpan Period()
        {
            var handle = CreateHandle();
            try
            {
                if (NativeCore.autd3_core_sampling_config_period_value(handle, out var value) != 0)
                {
                    throw new Autd3Exception("sampling config cannot be resolved to a period");
                }
                return TimeSpan.FromTicks((long)(value / 100));
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(handle);
            }
        }

        internal IntPtr CreateHandle()
        {
            var handle = _kind switch
            {
                Kind.Default => NativeCore.autd3_core_sampling_config_freq(4000f),
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
