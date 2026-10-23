using AUTD3;

namespace DocSamples.ApiCommandPattern;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var bank = PatternBank.B0;
        var transitionMode = TransitionMode.Immediate;

        var phases = geometry.PhaseBuffer();
        var intensities = Intensity.Max;

        // ANCHOR: api
        new Pattern(phases, intensities);

        new Pattern(bank, phases, intensities);

        new Pattern(
            bank: bank,
            phases: phases,
            intensities: intensities,
            transitionMode: transitionMode
        );
        // ANCHOR_END: api

        // ANCHOR: equivalent
        new WritePatternBuffer(
            bank: bank,
            startIndex: 0,
            patterns: new[] { phases },
            intensities: intensities
        );
        new ConfigPattern(
            bank: bank,
            config: new SamplingConfig(ushort.MaxValue),
            size: 1,
            loopBehavior: LoopBehavior.Infinite
        );
        new ActivatePatternBank(
            bank: bank,
            transitionMode: transitionMode
        );
        // ANCHOR_END: equivalent
    }
}
