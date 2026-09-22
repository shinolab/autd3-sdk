using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

namespace DocSamples.TutorialOutputIntensity;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        await using var client = await Client.OpenAsync(geometry, new EchocatLinkOption(), new ClientConfig());

        var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: pattern_intensity
        var phases = geometry.PhaseBuffer();
        Pattern.Focus(geometry, target, wavelength, phases);
        var intensity = new Intensity(0x80);
        // ANCHOR_END: pattern_intensity

        // ANCHOR: modulation
        var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(
            200 * Hz,
            new SineOption
            {
                Amplitude = 0xFF,
                Offset = 0x80,
                SamplingConfig = SamplingConfig.Freq4k,
            },
            modulation
        );
        // ANCHOR_END: modulation

        var builder = client.DatagramBuilder();
        builder.Push(new SetSilencer());
        builder.Push(new Pattern(phases, intensity));
        builder.Push(new Modulation(SamplingConfig.Freq4k, modulation));
        foreach (var frame in builder.Build())
        {
            await client.SendCheckedAsync(frame);
        }
    }
}
