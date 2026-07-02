using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

// HIDE
namespace AUTD3.DocSamples.ApiCommandClearExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, NopLink.Create(), new ClientConfig());

var builder = client.DatagramBuilder();
builder.Push(new Clear());
var frames = builder.Build();
foreach (var frame in frames)
{
    await client.SendCheckedAsync(frame);
}

await client.CloseAsync();
        // HIDE
    }
}
// HIDE_END
