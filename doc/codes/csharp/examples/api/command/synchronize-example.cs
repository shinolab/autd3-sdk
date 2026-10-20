using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandSynchronizeExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

await client.SendAsync(new Synchronize());
        // HIDE
    }
}
// HIDE_END
