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
        }

        private readonly Kind _kind;
        private readonly ushort _divide;

        private SamplingConfig(Kind kind, ushort divide)
        {
            _kind = kind;
            _divide = divide;
        }

        public static SamplingConfig Freq4k => new SamplingConfig(Kind.Freq4k, 0);

        public static SamplingConfig Freq40k => new SamplingConfig(Kind.Freq40k, 0);

        public static SamplingConfig Divide(ushort divide)
        {
            if (divide == 0)
            {
                throw new Autd3Exception("sampling divide must be >= 1");
            }
            return new SamplingConfig(Kind.Divide, divide);
        }

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

        internal IntPtr CreateHandle()
        {
            var handle = _kind switch
            {
                Kind.Freq4k => NativeCore.autd3_core_sampling_config_freq_4k(),
                Kind.Freq40k => NativeCore.autd3_core_sampling_config_freq_40k(),
                _ => NativeCore.autd3_core_sampling_config_divide(_divide),
            };
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create sampling config");
            }
            return handle;
        }
    }
}
