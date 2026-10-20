using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandModulationExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = await Client.OpenAsync(geometry, new UdpOption(), new ClientConfig());

var config = SamplingConfig.Freq4k;
var data = Modulation.ModulationBuffer();
Modulation.Sine(150 * Hz, new SineOption(), config, data);

await client.SendAsync(new Modulation(config, data));
        // HIDE
    }
}
// HIDE_END
