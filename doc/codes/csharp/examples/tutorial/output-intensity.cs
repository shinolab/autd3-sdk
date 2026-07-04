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

        var client = await Client.OpenAsync(geometry, new EtherCrabLinkOption(), new ClientConfig());

        var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: pattern_intensity
        var patterns = geometry.PatternBuffer();
        Pattern.Focus(
            geometry,
            target,
            wavelength,
            new FocusOption(intensity: new Intensity(0x80)),
            patterns
        );
        // ANCHOR_END: pattern_intensity

        var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(
            200 * Hz,
            new SineOption(
                amplitude: 0xFF,
                offset: 0x80,
                samplingConfig: SamplingConfig.Freq4k
            ),
            modulation
        );

        var builder = client.DatagramBuilder();
        builder.Push(new SetSilencer());
        builder.Push(new Pattern(patterns));
        builder.Push(new Modulation(SamplingConfig.Freq4k, modulation));
        foreach (var frame in builder.Build())
        {
            await client.SendCheckedAsync(frame);
        }

        await client.StopAsync();
        await client.CloseAsync();
    }
}
