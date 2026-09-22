using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandLowlevelPatternCompressedExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
await using var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

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

var builder = client.DatagramBuilder();
builder.Push(new WritePatternCompressed(
    bank: bank,
    index: 0,
    format: PatternCompression.PhaseHalf,
    intensity: Intensity.Max,
    patterns: patterns
));
builder.Push(new ConfigPattern(
    bank: bank,
    config: new StmConfig(1.0f * Hz).IntoSamplingConfig(patterns.Length),
    size: (uint)patterns.Length,
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
        // HIDE
    }
}
// HIDE_END
