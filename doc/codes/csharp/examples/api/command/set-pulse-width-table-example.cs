using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;

// HIDE
namespace DocSamples.ApiCommandSetPulseWidthTableExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
await using var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

var table = SetPulseWidthTable.DefaultTable();

var builder = client.DatagramBuilder();
builder.Push(new SetPulseWidthTable(table));
var frames = builder.Build();
foreach (var frame in frames)
{
    await client.SendCheckedAsync(frame);
}
        // HIDE
    }
}
// HIDE_END
