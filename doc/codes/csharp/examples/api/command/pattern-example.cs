using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandPatternExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

var phases = geometry.PhaseBuffer();
Pattern.Focus(
    geometry,
    geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm),
    Pattern.Wavelength(340.0f * m / s),
    phases
);

await client.SendAsync(new Pattern(phases, Intensity.Max));
        // HIDE
    }
}
// HIDE_END
