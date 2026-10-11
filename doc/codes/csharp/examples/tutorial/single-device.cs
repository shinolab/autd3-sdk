using System;
using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.TutorialSingleDevice;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
// Define a geometry consisting of a single AUTD3 device.
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

// Open the client.
await using var client = await Client.OpenAsync(
    geometry,
    new TransportOption(),
    new ClientConfig()
);

// Generate a focus 150 mm above the array center.
var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
var wavelength = Pattern.Wavelength(340.0f * m / s);
var phases = geometry.PhaseBuffer();
Pattern.Focus(
    geometry,
    target,
    wavelength,
    phases
);

// Apply a 200 Hz sine-wave AM.
var modulation = Modulation.ModulationBuffer();
Modulation.Sine(
    200 * Hz,
    new SineOption { SamplingConfig = SamplingConfig.Freq4k },
    modulation
);

await client.SendAsync(new SetSilencer());
await client.SendAsync(new Pattern(phases, Intensity.Max));
await client.SendAsync(new Modulation(SamplingConfig.Freq4k, modulation));

using var cts = new CancellationTokenSource();
Console.CancelKeyPress += (_, e) =>
{
    e.Cancel = true;
    cts.Cancel();
};

try
{
    await Task.Delay(Timeout.InfiniteTimeSpan, cts.Token);
}
catch (OperationCanceledException)
{
}
        // HIDE
    }
}
// HIDE_END
