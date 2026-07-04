using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiCommandLowlevelFociBuffer;

internal static class Sample
{
    internal static void Run()
    {
        var bank = PatternBank.B0;
        var points = Stm.Circle(Vector3.Zero, 30.0f * mm, 200, Vector3.UnitZ, Intensity.Max);
        var config = new SamplingConfig(points.Length * Hz);

        var indexOffset = 0u;
        // ANCHOR: write
        new WriteFociBuffer(
            bank: bank,
            indexOffset: indexOffset,
            points: points
        );
        // ANCHOR_END: write
        var size = (uint)points.Length;
        byte numFoci = 1;
        var soundSpeed = 340.0f * m / s;
        var loopBehavior = LoopBehavior.Infinite;
        // ANCHOR: config
        new ConfigFociStm(
            bank: bank,
            config: config,
            size: size,
            numFoci: numFoci,
            soundSpeed: soundSpeed,
            loopBehavior: loopBehavior
        );
        // ANCHOR_END: config
    }
}
