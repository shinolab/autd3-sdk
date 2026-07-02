using System.Numerics;
using AUTD3;

namespace AUTD3.DocSamples.ApiCommandPattern;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var bank = PatternBank.B0;

        var emissions = geometry.PatternBuffer();

        // ANCHOR: api
        new Pattern(emissions);

        new Pattern(emissions, bank);
        // ANCHOR_END: api

        // ANCHOR: equivalent
        new WritePatternBuffer(bank, 0, emissions);
        new ConfigPattern(bank, SamplingConfig.Divide(ushort.MaxValue), 1, PatternDataType.Raw, LoopBehavior.Infinite);
        new ChangePatternBank(bank, TransitionMode.Immediate);
        // ANCHOR_END: equivalent
    }
}
