using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandLowlevelBankExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

var phases = geometry.PhaseBuffer();
Pattern.Focus(
    geometry,
    geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm),
    Pattern.Wavelength(340.0f * m / s),
    phases
);

var bank = PatternBank.B0;

await client.SendAsync(new WritePatternBuffer(
    bank: bank,
    index: 0,
    phases: phases,
    intensities: Intensity.Max
));
await client.SendAsync(new ConfigPattern(
    bank: bank,
    config: new SamplingConfig(ushort.MaxValue),
    size: 1,
    loopBehavior: LoopBehavior.Infinite
));
await client.SendAsync(new ActivatePatternBank(
    bank: bank,
    transitionMode: TransitionMode.Immediate
));
        // HIDE
    }
}
// HIDE_END
