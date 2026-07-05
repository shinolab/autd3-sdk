using System.Collections.Generic;
using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiCommandFociStm;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var radius = 30.0f * mm;
        var numPoints = 200;
        var normal = Vector3.UnitZ;
        var intensity = Intensity.Max;
        var dst = new List<ControlPoints>();
        // ANCHOR: circle
        Stm.Circle(center, radius, numPoints, normal, intensity, dst);
        // ANCHOR_END: circle

        var start = center + new Vector3(-15.0f, 0.0f, 0.0f);
        var end = center + new Vector3(15.0f, 0.0f, 0.0f);
        // ANCHOR: line
        Stm.Line(start, end, numPoints, intensity, dst);
        // ANCHOR_END: line
        var points = dst.ToArray();
        var freq = 1.0f * Hz;
        var bank = PatternBank.B0;
        var soundSpeed = 340.0f * m / s;
        var loopBehavior = LoopBehavior.Infinite;
        var transitionMode = TransitionMode.Immediate;
        var option =
            // ANCHOR: option
            new FociStmOption(
                bank,
                soundSpeed,
                loopBehavior,
                transitionMode
            )
            // ANCHOR_END: option
            ;
        // ANCHOR: api
        new FociStm(new StmConfig(freq), points, option);
        // ANCHOR_END: api

        byte numFoci = 1;
        // ANCHOR: equivalent
        new WriteFociBuffer(
            bank: option.Bank,
            indexOffset: 0,
            points: points
        );
        new ConfigFociStm(
            bank: option.Bank,
            config: new SamplingConfig(points.Length * Hz),
            size: (uint)points.Length,
            numFoci: numFoci,
            soundSpeed: option.SoundSpeed,
            loopBehavior: option.LoopBehavior
        );
        new ChangePatternBank(
            bank: option.Bank,
            transitionMode: option.TransitionMode
        );
        // ANCHOR_END: equivalent
    }
}
