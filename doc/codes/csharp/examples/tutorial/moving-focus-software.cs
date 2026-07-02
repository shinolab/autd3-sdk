using System;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

namespace AUTD3.DocSamples.TutorialMovingFocusSoftware;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: loop
        var patterns = geometry.PatternBuffer();
        while (true)
        {
            foreach (var sign in new[] { 1.0f, -1.0f })
            {
                var target = center + new Vector3(sign * 20.0f, 0.0f, 0.0f);
                Pattern.Focus(geometry, target, wavelength, new FocusOption(), patterns);
                var builder = client.DatagramBuilder();
                builder.Push(new Pattern(patterns));
                foreach (var frame in builder.Build())
                {
                    await client.SendCheckedAsync(frame);
                }
                await Task.Delay(TimeSpan.FromSeconds(1));
            }
        }
        // ANCHOR_END: loop
    }
}
