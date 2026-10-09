using System.Numerics;
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
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

var phases = geometry.PhaseBuffer();
Pattern.Focus(
    geometry,
    geometry.Center + new Vector3(0.0f, 0.0f, 150.0f),
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
