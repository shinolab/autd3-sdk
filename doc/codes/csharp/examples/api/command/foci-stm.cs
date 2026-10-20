using System;
using System.Linq;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiCommandFociStm;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });
        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var radius = 30.0f * mm;
        var numPoints = 200;
        // ANCHOR: circle
        var points = Enumerable.Range(0, numPoints)
            .Select(i => 2.0f * MathF.PI * i / numPoints)
            .Select(theta => (ControlPoints)(center + Offset(MathF.Cos(theta) * radius, MathF.Sin(theta) * radius, 0.0f * mm)))
            .ToArray();
        // ANCHOR_END: circle
        var freq = 1.0f * Hz;
        var bank = PatternBank.B0;
        var soundSpeed = 340.0f * m / s;
        var loopBehavior = LoopBehavior.Infinite;
        var transitionMode = TransitionMode.Immediate;
        var option =
            // ANCHOR: option
            new FociStmOption
            {
                Bank = bank,
                SoundSpeed = soundSpeed,
                LoopBehavior = loopBehavior,
                TransitionMode = transitionMode,
            }
            // ANCHOR_END: option
            ;
        // ANCHOR: api
        new FociStm(freq, points, option);
        // ANCHOR_END: api

        byte numFoci = 1;
        // ANCHOR: equivalent
        new WriteFociBuffer(
            bank: option.Bank,
            startIndex: 0,
            points: points
        );
        new ConfigFociStm(
            bank: option.Bank,
            config: new StmConfig(freq).IntoSamplingConfig(points.Length),
            size: (uint)points.Length,
            numFoci: numFoci,
            soundSpeed: option.SoundSpeed,
            loopBehavior: option.LoopBehavior
        );
        new ActivatePatternBank(
            bank: option.Bank,
            transitionMode: option.TransitionMode
        );
        // ANCHOR_END: equivalent
    }
}
