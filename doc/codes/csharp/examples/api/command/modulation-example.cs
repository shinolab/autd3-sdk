using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandModulationExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
await using var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

var data = Modulation.ModulationBuffer();
Modulation.Sine(150 * Hz, new SineOption(), data);

var builder = client.DatagramBuilder();
builder.Push(new Modulation(SamplingConfig.Freq4k, data));
var frames = builder.Build();
foreach (var frame in frames)
{
    await client.SendCheckedAsync(frame);
}
        // HIDE
    }
}
// HIDE_END
