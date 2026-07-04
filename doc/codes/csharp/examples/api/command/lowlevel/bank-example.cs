using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandLowlevelBankExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

var emissions = geometry.PatternBuffer();
Pattern.Focus(
    geometry,
    geometry.Center + new Vector3(0.0f, 0.0f, 150.0f),
    Pattern.Wavelength(340.0f * m / s),
    new FocusOption(),
    emissions
);

var bank = PatternBank.B0;

var builder = client.DatagramBuilder();
builder.Push(new WritePatternBuffer(
    bank: bank,
    index: 0,
    emissions: emissions
));
builder.Push(new ConfigPattern(
    bank: bank,
    config: new SamplingConfig(ushort.MaxValue),
    size: 1,
    loopBehavior: LoopBehavior.Infinite
));
builder.Push(new ChangePatternBank(
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
