using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandLowlevelModBankExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

var data = Modulation.ModulationBuffer();
Modulation.Sine(150 * Hz, new SineOption(), data);

var bank = ModulationBank.B0;

var builder = client.DatagramBuilder();
builder.Push(new WriteModulationBuffer(
    bank: bank,
    offset: 0,
    data: data
));
builder.Push(new ConfigModulation(
    bank: bank,
    config: SamplingConfig.Freq4k,
    size: (uint)data.Length,
    loopBehavior: LoopBehavior.Infinite
));
builder.Push(new ChangeModulationBank(
    bank: bank,
    transitionMode: TransitionMode.Immediate
));
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
