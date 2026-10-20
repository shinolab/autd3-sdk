using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandOutputMaskExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = await Client.OpenAsync(geometry, new TransportOption());
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

var masks = geometry.OutputMaskBuffer();

await client.SendAsync(new SetOutputMask(masks));
        // HIDE
    }
}
// HIDE_END
