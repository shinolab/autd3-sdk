using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;
using AUTD3.Link;

namespace DocSamples.TutorialLoop;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        await using var client = await Client.OpenAsync(geometry, new EchocatLinkOption(), new ClientConfig());

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var dst = new List<ControlPoints>();
        Stm.Circle(center, 30.0f * mm, 20, Vector3.UnitZ, Intensity.Max, dst);
        var foci = dst.ToArray();

        {
        // ANCHOR: infinite
        // By default the playback loops infinitely; B0 keeps circling the focus.
        var b = client.DatagramBuilder();
        b.Push(new FociStm(50.0f * Hz, foci, new FociStmOption()));
        foreach (var frame in b.Build())
        {
            await client.SendCheckedAsync(frame);
        }
        // ANCHOR_END: infinite
        }

        // ANCHOR: finite
        // Play the circular motion only 3 times, then stop.
        // A finite loop (and non-immediate transition) only fires when switching to a
        // different bank, so write to bank B1 instead of the current B0.
        var builder = client.DatagramBuilder();
        builder.Push(new FociStm(
            50.0f * Hz,
            foci,
            new FociStmOption(
                loopBehavior: LoopBehavior.Finite(3),
                bank: PatternBank.B1,
                transitionMode: TransitionMode.SyncIdx
            )
        ));
        foreach (var frame in builder.Build())
        {
            await client.SendCheckedAsync(frame);
        }
        // ANCHOR_END: finite
    }
}
