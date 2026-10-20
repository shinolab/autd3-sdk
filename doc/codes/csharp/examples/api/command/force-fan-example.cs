using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandForceFanExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = await Client.OpenAsync(geometry, new UdpOption(), new ClientConfig());

await client.SendAsync(new ForceFan(true));
        // HIDE
    }
}
// HIDE_END
