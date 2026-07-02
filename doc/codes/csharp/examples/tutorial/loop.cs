using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

namespace AUTD3.DocSamples.TutorialLoop;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var foci = Stm.Circle(center, 30.0f, 20, Vector3.UnitZ, Intensity.Max);

        // By default the playback loops infinitely; B0 keeps circling the focus.
        var builder = client.DatagramBuilder();
        builder.Push(new FociStm(StmConfig.Freq(50.0f), foci, new FociStmOption()));
        foreach (var frame in builder.Build())
        {
            await client.SendCheckedAsync(frame);
        }

        // ANCHOR: finite
        // Play the circular motion only 3 times, then stop.
        // A finite loop (and non-immediate transition) only fires when switching to a
        // different bank, so write to bank B1 instead of the current B0.
        var builder2 = client.DatagramBuilder();
        builder2.Push(new FociStm(
            StmConfig.Freq(50.0f),
            foci,
            new FociStmOption(
                bank: PatternBank.B1,
                loopBehavior: LoopBehavior.Finite(3),
                transitionMode: TransitionMode.SyncIdx
            )
        ));
        foreach (var frame in builder2.Build())
        {
            await client.SendCheckedAsync(frame);
        }
        // ANCHOR_END: finite

        await client.CloseAsync();
    }
}
