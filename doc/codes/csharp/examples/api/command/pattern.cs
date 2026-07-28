using System.Numerics;
using AUTD3;

namespace DocSamples.ApiCommandPattern;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var bank = PatternBank.B0;
        var transitionMode = TransitionMode.Immediate;

        var emissions = geometry.PatternBuffer();

        // ANCHOR: api
        new Pattern(emissions);

        new Pattern(bank, emissions);

        new Pattern(
            bank: bank,
            emissions: emissions,
            transitionMode: transitionMode
        );
        // ANCHOR_END: api

        // ANCHOR: equivalent
        new WritePatternBuffer(
            bank: bank,
            index: 0,
            emissions: emissions
        );
        new ConfigPattern(
            bank: bank,
            config: new SamplingConfig(ushort.MaxValue),
            size: 1,
            loopBehavior: LoopBehavior.Infinite
        );
        new ChangePatternBank(
            bank: bank,
            transitionMode: transitionMode
        );
        // ANCHOR_END: equivalent
    }
}
