using System;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;
using AUTD3.Link;

namespace DocSamples.TutorialSilencer;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        await using var client = await Client.OpenAsync(geometry, new EchocatLinkOption(), new ClientConfig());

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var radius = 30.0f;

        {
            // ANCHOR: disable
            var foci = new ControlPoints[20];
            for (var i = 0; i < 20; i++)
            {
                var theta = 2.0f * MathF.PI * i / 20.0f;
                var p = center + new Vector3(radius * MathF.Cos(theta), radius * MathF.Sin(theta), 0.0f);
                foci[i] = new ControlPoints(new[] { new ControlPoint(p) }, Intensity.Max);
            }
            var builder = client.DatagramBuilder();
            builder.Push(SetSilencer.Disable());
            builder.Push(new FociStm(50.0f * Hz, foci));
            foreach (var frame in builder.Build())
            {
                await client.SendCheckedAsync(frame);
            }
            // ANCHOR_END: disable
        }

        {
            // ANCHOR: err
            var foci = new ControlPoints[40];
            for (var i = 0; i < 40; i++)
            {
                var theta = 2.0f * MathF.PI * i / 40.0f;
                var p = center + new Vector3(radius * MathF.Cos(theta), radius * MathF.Sin(theta), 0.0f);
                foci[i] = new ControlPoints(new[] { new ControlPoint(p) }, Intensity.Max);
            }
            var builder = client.DatagramBuilder();
            builder.Push(new SetSilencer());
            builder.Push(new FociStm(50.0f * Hz, foci));
            foreach (var frame in builder.Build())
            {
                await client.SendCheckedAsync(frame);
            }
            // ANCHOR_END: err
        }

        {
            // ANCHOR: workaround
            var foci = new ControlPoints[40];
            for (var i = 0; i < 40; i++)
            {
                var theta = 2.0f * MathF.PI * i / 40.0f;
                var p = center + new Vector3(radius * MathF.Cos(theta), radius * MathF.Sin(theta), 0.0f);
                foci[i] = new ControlPoints(new[] { new ControlPoint(p) }, Intensity.Max);
            }
            var builder = client.DatagramBuilder();
            builder.Push(new SetSilencer(new FixedCompletionTime(
                intensity: TimeSpan.FromMicroseconds(500),
                phase: TimeSpan.FromMicroseconds(500),
                strictMode: true
            )));
            builder.Push(new FociStm(50.0f * Hz, foci));
            foreach (var frame in builder.Build())
            {
                await client.SendCheckedAsync(frame);
            }
            // ANCHOR_END: workaround
        }
    }
}
