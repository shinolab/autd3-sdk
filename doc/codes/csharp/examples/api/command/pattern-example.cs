using System.Numerics;
using System.Threading;
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
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
var (driver, connector) = Driver.Open(emulator.Option(), geometry.NumDevices);
new Thread(driver.Run) { IsBackground = true }.Start();
await using var client = await Client.OpenAsync(geometry, connector, new ClientConfig());

var phases = geometry.PhaseBuffer();
Pattern.Focus(
    geometry,
    geometry.Center + new Vector3(0.0f, 0.0f, 150.0f),
    Pattern.Wavelength(340.0f * m / s),
    phases
);

var builder = client.DatagramBuilder();
builder.Push(new Pattern(phases, Intensity.Max));
var frames = builder.Build();
foreach (var frame in frames)
{
    await client.SendCheckedAsync(frame);
}
        // HIDE
    }
}
// HIDE_END
