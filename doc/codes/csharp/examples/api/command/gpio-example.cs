using System.Numerics;
using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandGpioExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

var builder = client.DatagramBuilder();
builder.Push(new SetGpioOut(new[]
{
    GpioOut.PatternBank,
    GpioOut.Thermo,
    GpioOut.PwmOut(0),
    GpioOut.Off,
}));
var frames = builder.Build();
foreach (var frame in frames)
{
    await client.SendCheckedAsync(frame);
}
        // HIDE
    }
}
// HIDE_END
