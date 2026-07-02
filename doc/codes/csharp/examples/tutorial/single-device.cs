using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.TutorialSingleDevice;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
// Define a geometry consisting of a single AUTD3 device.
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

// Open the client over an EtherCrab link.
var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

// Generate a focus 150 mm above the array center.
var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
var wavelength = Pattern.Wavelength(340.0f * m / s);
var patterns = geometry.PatternBuffer();
Pattern.Focus(geometry, target, wavelength, new FocusOption(), patterns);

// Apply a 200 Hz sine-wave AM.
var modulation = Modulation.ModulationBuffer();
Modulation.Sine(200 * Hz, new SineOption(samplingConfig: SamplingConfig.Freq4k), modulation);

var builder = client.DatagramBuilder();
builder.Push(new SetSilencer());
builder.Push(new Pattern(patterns));
builder.Push(new Modulation(SamplingConfig.Freq4k, modulation));
var frames = builder.Build();
foreach (var frame in frames)
{
    await client.SendCheckedAsync(frame);
}

await Task.Delay(Timeout.InfiniteTimeSpan);

await client.StopAsync();
await client.CloseAsync();
        // HIDE
    }
}
// HIDE_END
