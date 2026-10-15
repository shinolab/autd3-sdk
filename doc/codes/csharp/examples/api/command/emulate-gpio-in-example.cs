using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandEmulateGpioInExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

await client.SendAsync(new EmulateGpioIn(new[] { true, false, false, false }));
        // HIDE
    }
}
// HIDE_END
