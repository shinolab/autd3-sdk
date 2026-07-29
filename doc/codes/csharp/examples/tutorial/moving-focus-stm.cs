using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;
using AUTD3.Link;

namespace DocSamples.TutorialMovingFocusStm;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var client = await Client.OpenAsync(geometry, new EchocatLinkOption(), new ClientConfig());

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);

        // ANCHOR: stm
        var points = new[]
        {
            new ControlPoints(new[] { new ControlPoint(center + new Vector3(20.0f, 0.0f, 0.0f)) }),
            new ControlPoints(new[] { new ControlPoint(center + new Vector3(-20.0f, 0.0f, 0.0f)) }),
        };
        var builder = client.DatagramBuilder();
        builder.Push(new FociStm(0.5f * Hz, points));
        foreach (var frame in builder.Build())
        {
            await client.SendCheckedAsync(frame);
        }
        // ANCHOR_END: stm

        await client.CloseAsync();
    }
}
