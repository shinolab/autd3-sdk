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
var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

var wavelength = Pattern.Wavelength(340.0f * m / s);
var offsets = new[] { -30.0f, -10.0f, 10.0f, 30.0f };
var patterns = new PatternBuffer[offsets.Length];
for (var i = 0; i < offsets.Length; i++)
{
    var buffer = geometry.PatternBuffer();
    Pattern.Focus(
        geometry,
        geometry.Center + new Vector3(offsets[i], 0.0f, 150.0f),
        wavelength,
        new FocusOption(),
        buffer
    );
    patterns[i] = buffer;
}

var bank = PatternBank.B0;

var builder = client.DatagramBuilder();
builder.Push(new WritePatternCompressed(
    bank,
    0,
    PatternCompression.PhaseHalf,
    patterns
));
builder.Push(new ConfigPattern(
    bank,
    new SamplingConfig(patterns.Length * Hz),
    (uint)patterns.Length,
    LoopBehavior.Infinite
));
builder.Push(new ChangePatternBank(
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
