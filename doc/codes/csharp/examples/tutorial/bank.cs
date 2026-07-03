using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

namespace AUTD3.DocSamples.TutorialBank;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        var wavelength = Pattern.Wavelength(340.0f * m / s);

        // ANCHOR: switch
        // Write focus A to bank B0 and play it.
        var targetA = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var patA = geometry.PatternBuffer();
        Pattern.Focus(
            geometry,
            targetA,
            wavelength,
            new FocusOption(),
            patA
        );
        var builder = client.DatagramBuilder();
        builder.Push(new Pattern(patA, PatternBank.B0));
        foreach (var frame in builder.Build())
        {
            await client.SendCheckedAsync(frame);
        }

        // Write focus B to bank B1, which is not currently playing, then switch to B1.
        // B0 keeps playing cleanly while B1 is being written (double buffering).
        var targetB = geometry.Center + new Vector3(0.0f, 30.0f, 150.0f);
        var patB = geometry.PatternBuffer();
        Pattern.Focus(
            geometry,
            targetB,
            wavelength,
            new FocusOption(),
            patB
        );
        var builder2 = client.DatagramBuilder();
        builder2.Push(new Pattern(patB, PatternBank.B1));
        foreach (var frame in builder2.Build())
        {
            await client.SendCheckedAsync(frame);
        }
        // ANCHOR_END: switch

        await client.CloseAsync();
    }
}
