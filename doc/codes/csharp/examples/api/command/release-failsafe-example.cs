using System.Linq;
using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandReleaseFailsafeExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

var states = await client.ReadFpgaStateAsync();
if (states.Any(state => state.IsFailsafeActive))
{
    await client.SendAsync(new ReleaseFailsafe());
}
        // HIDE
    }
}
// HIDE_END
