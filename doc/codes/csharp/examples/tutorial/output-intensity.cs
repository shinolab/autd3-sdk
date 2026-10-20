using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.TutorialOutputIntensity;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        await using var client = Client.Open(geometry, new TransportOption());
        await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

        var target = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: pattern_intensity
        var phases = geometry.PhaseBuffer();
        Pattern.Focus(geometry, target, wavelength, phases);
        var intensity = new Intensity(0x80);
        // ANCHOR_END: pattern_intensity

        // ANCHOR: modulation
        var samplingConfig = SamplingConfig.Freq4k;
        var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(
            200 * Hz,
            new SineOption
            {
                Amplitude = 0xFF,
                Offset = 0x80,
            },
            samplingConfig,
            modulation
        );
        // ANCHOR_END: modulation

        await client.SendAsync(new SetSilencer());
        await client.SendAsync(new Pattern(phases, intensity));
        await client.SendAsync(new Modulation(samplingConfig, modulation));
    }
}
