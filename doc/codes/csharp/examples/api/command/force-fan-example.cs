using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandForceFanExample;

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

var builder = client.DatagramBuilder();
builder.Push(new ForceFan(true));
var frames = builder.Build();
foreach (var frame in frames)
{
    await client.SendCheckedAsync(frame);
}
        // HIDE
    }
}
// HIDE_END
