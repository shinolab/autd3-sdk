using System;
using System.Collections.Generic;
using System.CommandLine.Parsing;
using System.Linq;

namespace TwincatCli
{
    internal enum TwinCATVersion
    {
        Build4024,
        Build4026,
    }

    internal static class TwinCATVersionParser
    {
        private static Dictionary<string, TwinCATVersion> _availables = new Dictionary<string, TwinCATVersion>()
        {
            { "4024", TwinCATVersion.Build4024 },
            { "4026", TwinCATVersion.Build4026 },
        };

        internal static IEnumerable<string> AvailableVersions()
        {
            return _availables.OrderBy(x => x.Key).Select(x => x.Key);
        }

        internal static TwinCATVersion Parse(OptionResult result)
        {
            var availables = string.Join(", ", AvailableVersions());
            var version = result.GetValueOrDefault<string>().ToLowerInvariant();
            if (_availables.TryGetValue(version, out var v))
                return v;
            else
                throw new ArgumentException($"Invalid CPU base time '{version}'. Available options: {availables}.");
        }
    }
}