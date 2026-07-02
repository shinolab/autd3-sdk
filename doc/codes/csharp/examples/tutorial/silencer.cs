using System;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

namespace AUTD3.DocSamples.TutorialSilencer;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);

        {
            // ANCHOR: disable
            var foci = Stm.Circle(center, 30.0f, 20, Vector3.UnitZ, Intensity.Max);
            var builder = client.DatagramBuilder();
            builder.Push(SetSilencer.Disable());
            builder.Push(new FociStm(StmConfig.Freq(50.0f), foci, new FociStmOption()));
            foreach (var frame in builder.Build())
            {
                await client.SendCheckedAsync(frame);
            }
            // ANCHOR_END: disable
        }

        {
            // ANCHOR: err
            var foci = Stm.Circle(center, 30.0f, 40, Vector3.UnitZ, Intensity.Max);
            var builder = client.DatagramBuilder();
            builder.Push(new SetSilencer());
            builder.Push(new FociStm(StmConfig.Freq(50.0f), foci, new FociStmOption()));
            foreach (var frame in builder.Build())
            {
                await client.SendCheckedAsync(frame);
            }
            // ANCHOR_END: err
        }

        {
            // ANCHOR: workaround
            var foci = Stm.Circle(center, 30.0f, 40, Vector3.UnitZ, Intensity.Max);
            var builder = client.DatagramBuilder();
            builder.Push(new SetSilencer(new FixedCompletionTime(
                intensity: TimeSpan.FromMicroseconds(500),
                phase: TimeSpan.FromMicroseconds(500),
                strictMode: true
            )));
            builder.Push(new FociStm(StmConfig.Freq(50.0f), foci, new FociStmOption()));
            foreach (var frame in builder.Build())
            {
                await client.SendCheckedAsync(frame);
            }
            // ANCHOR_END: workaround
        }

        await client.CloseAsync();
    }
}
