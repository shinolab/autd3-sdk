using System;
using System.Linq;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiCommandLowlevelFociBuffer;

internal static class Sample
{
    internal static void Run()
    {
        var bank = PatternBank.B0;
        var points = Enumerable.Range(0, 200)
            .Select(i => 2.0f * MathF.PI * i / 200)
            .Select(theta => (ControlPoints)Offset(MathF.Cos(theta) * 30.0f * mm, MathF.Sin(theta) * 30.0f * mm, 0.0f * mm))
            .ToArray();
        var config = new SamplingConfig(1.0f * Hz * points.Length);

        var startIndex = 0u;
        // ANCHOR: write
        new WriteFociBuffer(
            bank: bank,
            startIndex: startIndex,
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
