using System.Numerics;
using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandSetPulseWidthTableExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

var table = SetPulseWidthTable.EmptyTable();
for (var i = 0; i < table.Length; i++)
{
    table[i] = new PulseWidth((ushort)i);
}

await client.SendAsync(new SetPulseWidthTable(table));
        // HIDE
    }
}
// HIDE_END
