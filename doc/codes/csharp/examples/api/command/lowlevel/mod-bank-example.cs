using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandLowlevelModBankExample;

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

var bank = ModulationBank.B0;

await client.SendAsync(new WriteModulationBuffer(
    bank: bank,
    offset: 0,
    data: data
));
await client.SendAsync(new ConfigModulation(
    bank: bank,
    config: SamplingConfig.Freq4k,
    size: (uint)data.Length,
    loopBehavior: LoopBehavior.Infinite
));
await client.SendAsync(new ActivateModulationBank(
    bank: bank,
    transitionMode: TransitionMode.Immediate
));
        // HIDE
    }
}
// HIDE_END
