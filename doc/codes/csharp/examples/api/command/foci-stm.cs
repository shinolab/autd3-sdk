using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiCommandFociStm;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });
        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var radius = 30.0f;
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
                soundSpeedMS: 340.0f,
                loopBehavior: LoopBehavior.Infinite,
                transitionMode: TransitionMode.Immediate
            )
            // ANCHOR_END: option
            ;
        // ANCHOR: api
        new FociStm(StmConfig.FromFreq(freq), points, option);
        // ANCHOR_END: api

        byte numFoci = 1;
        // ANCHOR: equivalent
        new WriteFociBuffer(
            option.Bank,
            0,
            points
        );
        new ConfigPattern(
            option.Bank,
            SamplingConfig.FromFreq(points.Length * Hz),
            (uint)points.Length,
            PatternDataType.Foci(numFoci, 340),
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
