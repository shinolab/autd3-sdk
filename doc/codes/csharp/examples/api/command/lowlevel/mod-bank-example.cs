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
    bank,
    0,
    data
));
builder.Push(new ConfigModulation(
    bank,
    SamplingConfig.Freq4k,
    (uint)data.Length,
    LoopBehavior.Infinite
));
builder.Push(new ChangeModulationBank(
    bank,
    TransitionMode.Immediate
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
