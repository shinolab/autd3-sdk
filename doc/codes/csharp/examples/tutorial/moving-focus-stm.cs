using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.TutorialMovingFocusStm;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        using var client = Client.Open(geometry, new TransportOption());
        await client.SendAsync(new Clear());
        await client.SendAsync(new Synchronize());

        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);

        // ANCHOR: stm
        var points = new[]
        {
            (ControlPoints)(center + Offset(20.0f * mm, 0.0f * mm, 0.0f * mm)),
            (ControlPoints)(center + Offset(-20.0f * mm, 0.0f * mm, 0.0f * mm)),
        };
        var config = new SamplingConfig(1.0f * Hz);
        await client.SendAsync(new FociStm(config, points, new FociStmOption()));
        // ANCHOR_END: stm

        await client.SendAsync(new Pattern(geometry.PhaseBuffer(), Intensity.Min));
        client.Close();
    }
}
