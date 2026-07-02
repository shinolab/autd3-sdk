using System;

namespace AUTD3
{


    public readonly struct SamplingConfig
    {
        private enum Kind : byte
        {
            Freq4k,
            Freq40k,
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

        public static SamplingConfig Freq4k => new SamplingConfig(Kind.Freq4k, 0, 0f, 0);

        public static SamplingConfig Freq40k => new SamplingConfig(Kind.Freq40k, 0, 0f, 0);

        public static SamplingConfig Divide(ushort divide)
        {
            if (divide == 0)
            {
                throw new Autd3Exception("sampling divide must be >= 1");
            }
            return new SamplingConfig(Kind.Divide, divide, 0f, 0);
        }

        public static SamplingConfig FromFreq(Freq freq)
        {
            var kind = freq.Mode == Freq.FreqMode.Nearest ? Kind.FreqNearest : Kind.Freq;
            return new SamplingConfig(kind, 0, freq.Hz, 0);
        }

        public static SamplingConfig FromPeriod(TimeSpan period) =>
            new SamplingConfig(Kind.Period, 0, 0f, (ulong)(period.Ticks * 100));

        public static SamplingConfig FromPeriodNearest(TimeSpan period) =>
            new SamplingConfig(Kind.PeriodNearest, 0, 0f, (ulong)(period.Ticks * 100));

        public ushort DivideValue()
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

        public Freq FreqValue()
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

        public TimeSpan PeriodValue()
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
                Kind.Freq4k => NativeCore.autd3_core_sampling_config_freq_4k(),
                Kind.Freq40k => NativeCore.autd3_core_sampling_config_freq_40k(),
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
