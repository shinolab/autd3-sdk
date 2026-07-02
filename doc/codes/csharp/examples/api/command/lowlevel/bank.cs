using System.Numerics;
using AUTD3;

namespace AUTD3.DocSamples.ApiCommandLowlevelBank;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var bank = PatternBank.B0;
        ushort index = 0;
        var emissions = geometry.PatternBuffer();
        // ANCHOR: write
        new WritePatternBuffer(bank, index, emissions);
        // ANCHOR_END: write
        var config = SamplingConfig.Freq4k;
        var size = 1u;
        var loopBehavior = LoopBehavior.Infinite;
        // ANCHOR: config
        new ConfigPattern(bank, config, size, PatternDataType.Raw, loopBehavior);
        // ANCHOR_END: config
        var transitionMode = TransitionMode.Immediate;
        // ANCHOR: change
        new ChangePatternBank(bank, transitionMode);
        // ANCHOR_END: change

        var p0 = geometry.PatternBuffer();
        var p1 = geometry.PatternBuffer();
        var p2 = geometry.PatternBuffer();
        var p3 = geometry.PatternBuffer();
        var patterns = new[] { p0, p1, p2, p3 };
        var compressedIndex = 0u;
        var format = PatternCompression.PhaseHalf;
        // ANCHOR: compressed
        new WritePatternCompressed(bank, compressedIndex, format, patterns);
        // ANCHOR_END: compressed
    }
}
