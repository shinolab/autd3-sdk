using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandLowlevelPatternPhaseExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

var wavelength = Pattern.Wavelength(340.0f * m / s);
var offsets = new[] { -30.0f, -10.0f, 10.0f, 30.0f };
var patterns = new PhaseBuffer[offsets.Length];
for (var i = 0; i < offsets.Length; i++)
{
    var buffer = geometry.PhaseBuffer();
    Pattern.Focus(
        geometry,
        geometry.Center + new Vector3(offsets[i], 0.0f, 150.0f),
        wavelength,
        buffer
    );
    patterns[i] = buffer;
}

var bank = PatternBank.B0;

await client.SendAsync(new WritePatternPhase(
    bank: bank,
    index: 0,
    depth: PhaseDepth.Bits4,
    intensity: Intensity.Max,
    patterns: patterns
));
await client.SendAsync(new ConfigPattern(
    bank: bank,
    config: new StmConfig(1.0f * Hz).IntoSamplingConfig(patterns.Length),
    size: (uint)patterns.Length,
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
