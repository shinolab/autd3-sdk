using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;

// HIDE
namespace DocSamples.ApiCommandPhaseCorrectionExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

var phases = new Phase[geometry.NumDevices][];
for (var i = 0; i < geometry.NumDevices; i++)
{
    var count = geometry[i].NumTransducers;
    phases[i] = new Phase[count];
    for (var t = 0; t < count; t++)
    {
        phases[i][t] = Phase.Zero;
    }
}

var builder = client.DatagramBuilder();
builder.Push(new SetPhaseCorrection(phases));
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
