using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.TutorialMovingFocusStm;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());
        await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);

        // ANCHOR: stm
        var points = new[]
        {
            (ControlPoints)(center + Offset(20.0f * mm, 0.0f * mm, 0.0f * mm)),
            (ControlPoints)(center + Offset(-20.0f * mm, 0.0f * mm, 0.0f * mm)),
        };
        await client.SendAsync(new FociStm(0.5f * Hz, points, new FociStmOption()));
        // ANCHOR_END: stm
    }
}
