using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiBasicsSend;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(), new Autd3() });
        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

        var wavelength = Pattern.Wavelength(340.0f * m / s);

        var leftTarget = geometry.Center + Offset(-40.0f * mm, 0.0f * mm, 150.0f * mm);
        var left = geometry.PhaseBuffer();
        Pattern.Focus(geometry, leftTarget, wavelength, left);

        var rightTarget = geometry.Center + Offset(40.0f * mm, 0.0f * mm, 150.0f * mm);
        var right = geometry.PhaseBuffer();
        Pattern.Focus(geometry, rightTarget, wavelength, right);

        var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(150 * Hz, new SineOption(), SamplingConfig.Freq4k, modulation);
        {
        // ANCHOR: api
        await client.SendAsync(new SetSilencer());

        var done = await client.SendStreamingAsync(new Modulation(SamplingConfig.Freq4k, modulation));
        await done;

        using var frames = Frames.Encode(geometry, new Pattern(left, Intensity.Max));
        foreach (var frame in frames)
        {
            (await await client.SendFrameAsync(frame)).Check();
        }
        // ANCHOR_END: api
        }
        
        {
        // ANCHOR: each
        await client.SendAsync(Command.Each(device => device.Idx % 2 == 0
            ? new Pattern(left, Intensity.Max)
            : new Pattern(right, Intensity.Max)));
        // ANCHOR_END: each
        }
    }
}
