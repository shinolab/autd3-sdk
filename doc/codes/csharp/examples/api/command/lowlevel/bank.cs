using System.Numerics;
using AUTD3;

namespace DocSamples.ApiCommandLowlevelBank;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var bank = PatternBank.B0;
        ushort index = 0;
        var emissions = geometry.PatternBuffer();
        // ANCHOR: write
        new WritePatternBuffer(
            bank: bank,
            index: index,
            emissions: emissions
        );
        // ANCHOR_END: write
        var config = SamplingConfig.Freq4k;
        var size = 1u;
        var loopBehavior = LoopBehavior.Infinite;
        // ANCHOR: config
        new ConfigPattern(
            bank: bank,
            config: config,
            size: size,
            loopBehavior: loopBehavior
        );
        // ANCHOR_END: config
        var transitionMode = TransitionMode.Immediate;
        // ANCHOR: change
        new ChangePatternBank(
            bank: bank,
            transitionMode: transitionMode
        );
        // ANCHOR_END: change

        var p0 = geometry.PatternBuffer();
        var p1 = geometry.PatternBuffer();
        var p2 = geometry.PatternBuffer();
        var p3 = geometry.PatternBuffer();
        var patterns = new[] { p0, p1, p2, p3 };
        var compressedIndex = 0u;
        var format = PatternCompression.PhaseHalf;
        // ANCHOR: compressed
        new WritePatternCompressed(
            bank: bank,
            index: compressedIndex,
            format: format,
            patterns: patterns
        );
        // ANCHOR_END: compressed
    }
}
