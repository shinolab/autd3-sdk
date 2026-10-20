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
await using var client = Client.Open(geometry, new TransportOption());
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

var states = await client.ReadFpgaStateAsync();
if (states.Any(state => state.IsFailsafeActive))
{
    await client.SendAsync(new ReleaseFailsafe());
}
        // HIDE
    }
}
// HIDE_END
