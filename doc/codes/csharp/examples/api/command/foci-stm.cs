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
        // ANCHOR: circle
        var points = Stm.Circle(center, radius, numPoints, normal, intensity);
        // ANCHOR_END: circle

        var start = center + new Vector3(-15.0f, 0.0f, 0.0f);
        var end = center + new Vector3(15.0f, 0.0f, 0.0f);
        // ANCHOR: line
        var linePoints = Stm.Line(start, end, numPoints, intensity);
        // ANCHOR_END: line
        var freq = 1.0f * Hz;
        var option =
            // ANCHOR: option
            new FociStmOption(
                bank: PatternBank.B0,
                soundSpeed: 340.0f * m / s,
                loopBehavior: LoopBehavior.Infinite,
                transitionMode: TransitionMode.Immediate
            )
            // ANCHOR_END: option
            ;
        // ANCHOR: api
        new FociStm(new StmConfig(freq), points, option);
        // ANCHOR_END: api

        byte numFoci = 1;
        // ANCHOR: equivalent
        new WriteFociBuffer(
            option.Bank,
            0,
            points
        );
        new ConfigFociStm(
            option.Bank,
            new SamplingConfig(points.Length * Hz),
            (uint)points.Length,
            numFoci,
            option.SoundSpeed,
            option.LoopBehavior
        );
        new ChangePatternBank(
            option.Bank,
            option.TransitionMode
        );
        // ANCHOR_END: equivalent

        _ = linePoints;
    }
}
