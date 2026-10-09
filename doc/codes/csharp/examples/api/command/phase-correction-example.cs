using System.Numerics;
using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandPhaseCorrectionExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

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

await client.SendAsync(new SetPhaseCorrection(phases));
        // HIDE
    }
}
// HIDE_END
