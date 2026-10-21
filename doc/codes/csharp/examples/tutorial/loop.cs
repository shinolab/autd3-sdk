using System;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.TutorialLoop;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        await using var client = Client.Open(geometry, new TransportOption());
        await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var radius = (30.0f * mm).Mm;
        var foci = new ControlPoints[20];
        for (var i = 0; i < 20; i++)
        {
            var theta = 2.0f * MathF.PI * i / 20.0f;
            var p = center + new Vector3(radius * MathF.Cos(theta), radius * MathF.Sin(theta), 0.0f);
            foci[i] = new ControlPoints(new[] { (ControlPoint)p }, Intensity.Max);
        }

        {
        // ANCHOR: infinite
        // By default the playback loops infinitely; B0 keeps circling the focus.
        await client.SendAsync(new FociStm(new SamplingConfig(50.0f * Hz * foci.Length), foci, new FociStmOption()));
        // ANCHOR_END: infinite
        }

        // ANCHOR: finite
        // Play the circular motion only 3 times, then stop.
        // A finite loop (and non-immediate transition) only fires when switching to a
        // different bank, so write to bank B1 instead of the current B0.
        await client.SendAsync(new FociStm(
            new SamplingConfig(50.0f * Hz * foci.Length),
            foci,
            new FociStmOption
            {
                LoopBehavior = LoopBehavior.Finite(3),
                Bank = PatternBank.B1,
                TransitionMode = TransitionMode.SyncIdx,
            }
        ));
        // ANCHOR_END: finite
    }
}
