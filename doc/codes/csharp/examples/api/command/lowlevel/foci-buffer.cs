using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiCommandLowlevelFociBuffer;

internal static class Sample
{
    internal static void Run()
    {
        var bank = PatternBank.B0;
        var points = Stm.Circle(Vector3.Zero, 30.0f, 200, Vector3.UnitZ, Intensity.Max);
        var config = SamplingConfig.FromFreq(points.Length * Hz);

        var indexOffset = 0u;
        // ANCHOR: write
        new WriteFociBuffer(
            bank,
            indexOffset,
            points
        );
        // ANCHOR_END: write
        var size = (uint)points.Length;
        byte numFoci = 1;
        ushort soundSpeed = 340;
        var loopBehavior = LoopBehavior.Infinite;
        // ANCHOR: config
        new ConfigPattern(
            bank,
            config,
            size,
            PatternDataType.Foci(numFoci, soundSpeed),
            loopBehavior
        );
        // ANCHOR_END: config
    }
}
