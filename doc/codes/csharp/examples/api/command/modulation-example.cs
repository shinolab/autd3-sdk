using System.Numerics;
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
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

var data = Modulation.ModulationBuffer();
Modulation.Sine(150 * Hz, new SineOption(), data);

await client.SendAsync(new Modulation(SamplingConfig.Freq4k, data));
        // HIDE
    }
}
// HIDE_END
