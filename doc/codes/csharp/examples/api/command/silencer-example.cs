using System.Numerics;
using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandSilencerExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

await client.SendAsync(new SetSilencer());
        // HIDE
    }
}
// HIDE_END
