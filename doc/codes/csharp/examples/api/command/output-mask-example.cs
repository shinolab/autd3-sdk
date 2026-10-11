using System.Numerics;
using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandOutputMaskExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

var masks = new bool[geometry.NumDevices][];
for (var i = 0; i < geometry.NumDevices; i++)
{
    var count = geometry[i].NumTransducers;
    masks[i] = new bool[count];
    for (var t = 0; t < count; t++)
    {
        masks[i][t] = true;
    }
}

await client.SendAsync(new SetOutputMask(masks));
        // HIDE
    }
}
// HIDE_END
