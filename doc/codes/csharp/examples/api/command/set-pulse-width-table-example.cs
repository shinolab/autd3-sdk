using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

// HIDE
namespace AUTD3.DocSamples.ApiCommandSetPulseWidthTableExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, NopLink.Create(), new ClientConfig());

var table = PulseWidth.DefaultTable();

var builder = client.DatagramBuilder();
builder.Push(new SetPulseWidthTable(table));
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
