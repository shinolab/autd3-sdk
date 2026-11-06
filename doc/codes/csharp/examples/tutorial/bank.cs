using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.TutorialBank;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        using var client = Client.Open(geometry, new TransportOption());
        await client.SendAsync(new Clear());
        await client.SendAsync(new Synchronize());

        var wavelength = Pattern.Wavelength(340.0f * m / s);


        // ANCHOR: switch
        // Write focus A to bank B0 and play it.
        var targetA = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var patA = geometry.PhaseBuffer();
        Pattern.Focus(
            geometry,
            targetA,
            wavelength,
            patA
        );
        await client.SendAsync(new Pattern(PatternBank.B0, patA, Intensity.Max));

        // Write focus B to bank B1, which is not currently playing, then switch to B1.
        // B0 keeps playing cleanly while B1 is being written (double buffering).
        var targetB = geometry.Center + Offset(0.0f * mm, 30.0f * mm, 150.0f * mm);
        var patB = geometry.PhaseBuffer();
        Pattern.Focus(
            geometry,
            targetB,
            wavelength,
            patB
        );
        await client.SendAsync(new Pattern(PatternBank.B1, patB, Intensity.Max));
        // ANCHOR_END: switch

        await client.SendAsync(new Pattern(geometry.PhaseBuffer(), Intensity.Min));
    }
}
