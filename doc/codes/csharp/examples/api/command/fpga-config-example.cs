using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandFpgaConfigExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = await Client.OpenAsync(geometry, new TransportOption());
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

await client.SendAsync(new SetFpgaConfig(new FpgaConfig { SyncPhaseLock = false }));

await client.SendAsync(new SetFpgaConfig());
        // HIDE
    }
}
// HIDE_END
