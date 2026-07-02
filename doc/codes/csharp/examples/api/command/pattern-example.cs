using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.ApiCommandPatternExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, NopLink.Create(), new ClientConfig());

var emissions = geometry.PatternBuffer();
Pattern.Focus(
    geometry,
    geometry.Center + new Vector3(0.0f, 0.0f, 150.0f),
    Pattern.Wavelength(340.0f * m / s),
    new FocusOption(),
    emissions
);

var builder = client.DatagramBuilder();
builder.Push(new Pattern(emissions));
var frames = builder.Build();
foreach (var frame in frames)
{
    await client.SendCheckedAsync(frame);
}

await client.CloseAsync();
        // HIDE
    }
}
// HIDE_END
