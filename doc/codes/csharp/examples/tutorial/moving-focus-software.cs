using System;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.TutorialMovingFocusSoftware;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var client = await Client.OpenAsync(geometry, new TransportOption());
        await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: loop
        var phases = geometry.PhaseBuffer();
        while (true)
        {
            foreach (var sign in new[] { 1.0f, -1.0f })
            {
                var target = center + Offset(sign * 20.0f * mm, 0.0f * mm, 0.0f * mm);
                Pattern.Focus(
                    geometry,
                    target,
                    wavelength,
                    phases
                );
                await client.SendAsync(new Pattern(phases, Intensity.Max));
                await Task.Delay(TimeSpan.FromSeconds(1));
            }
        }
        // ANCHOR_END: loop
    }
}
