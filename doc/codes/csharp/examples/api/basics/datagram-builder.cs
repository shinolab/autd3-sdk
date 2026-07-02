using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiBasicsDatagramBuilder;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero), new Device(Vector3.Zero) });
        var client = await Client.OpenAsync(geometry, NopLink.Create(), new ClientConfig());

        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var option = new FocusOption();

        var leftTarget = geometry.Center + new Vector3(-40.0f, 0.0f, 150.0f);
        var left = geometry.PatternBuffer();
        Pattern.Focus(geometry, leftTarget, wavelength, option, left);

        var rightTarget = geometry.Center + new Vector3(40.0f, 0.0f, 150.0f);
        var right = geometry.PatternBuffer();
        Pattern.Focus(geometry, rightTarget, wavelength, option, right);

        var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(150 * Hz, new SineOption(), modulation);

        // ANCHOR: api
        var builder = client.DatagramBuilder();
        builder.Push(new SetSilencer());
        var frames = builder.Build();
        foreach (var frame in frames)
        {
            await client.SendCheckedAsync(frame);
        }
        // ANCHOR_END: api

        // ANCHOR: push_each
        var builder2 = client.DatagramBuilder();
        builder2.PushEach(device => device % 2 == 0
            ? new Pattern(left)
            : new Pattern(right));
        var frames2 = builder2.Build();
        // ANCHOR_END: push_each

        foreach (var frame in frames2)
        {
            await client.SendCheckedAsync(frame);
        }

        await client.CloseAsync();
    }
}
