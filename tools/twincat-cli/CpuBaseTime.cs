using System;
using System.Collections.Generic;
using System.CommandLine.Parsing;
using System.Linq;

namespace TwincatCli
{
    internal enum CpuBaseTime
    {
        T_1ms,
        T_500us,
        T_333us,
        T_250us,
        T_200us,
        T_125us,
        T_100us,
        T_83p3us,
        T_76p9us,
        T_71p4us,
        T_66p6us,
        T_62p5us,
        T_50us,
        None
    }

    internal static class CpuBaseTimeParser
    {
        private static Dictionary<string, CpuBaseTime> _availables = new Dictionary<string, CpuBaseTime>()
    {
        { "none", CpuBaseTime.None },
        { "1ms", CpuBaseTime.T_1ms },
        { "500us", CpuBaseTime.T_500us },
        { "333us", CpuBaseTime.T_333us },
        { "250us", CpuBaseTime.T_250us },
        { "200us", CpuBaseTime.T_200us },
        { "125us", CpuBaseTime.T_125us },
        { "100us", CpuBaseTime.T_100us },
        { "83.3us", CpuBaseTime.T_83p3us },
        { "76.9us", CpuBaseTime.T_76p9us },
        { "71.4us", CpuBaseTime.T_71p4us },
        { "66.6us", CpuBaseTime.T_66p6us },
        { "62.5us", CpuBaseTime.T_62p5us },
        { "50us", CpuBaseTime.T_50us },
    };

        internal static int ToValueUnitsOf100ns(CpuBaseTime cpuBaseTime)
        {
            switch (cpuBaseTime)
            {
                case CpuBaseTime.None: return 0;
                case CpuBaseTime.T_1ms: return 10000;
                case CpuBaseTime.T_500us: return 5000;
                case CpuBaseTime.T_333us: return 3333;
                case CpuBaseTime.T_250us: return 2500;
                case CpuBaseTime.T_200us: return 2000;
                case CpuBaseTime.T_125us: return 1250;
                case CpuBaseTime.T_100us: return 1000;
                case CpuBaseTime.T_83p3us: return 833;
                case CpuBaseTime.T_76p9us: return 769;
                case CpuBaseTime.T_71p4us: return 714;
                case CpuBaseTime.T_66p6us: return 666;
                case CpuBaseTime.T_62p5us: return 625;
                case CpuBaseTime.T_50us: return 500;
                default:
                    throw new ArgumentOutOfRangeException(nameof(cpuBaseTime), cpuBaseTime, null);
            }
        }

        internal static IEnumerable<string> AvailableTimes()
        {
            return _availables.OrderBy(x => ToValueUnitsOf100ns(x.Value)).Select(x => x.Key);
        }

        internal static CpuBaseTime Parse(OptionResult result)
        {
            var availableTime = string.Join(", ", AvailableTimes());
            var time = result.GetValueOrDefault<string>().ToLowerInvariant();
            if (_availables.TryGetValue(time, out var cpuBaseTime))
                return cpuBaseTime;
            else
                throw new ArgumentException($"Invalid CPU base time '{time}'. Available options: {availableTime}.");
        }
    }
}