using System;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandPatternStmExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
var wavelength = Pattern.Wavelength(340.0f * m / s);
var patterns = new PatternBuffer[200];
for (var i = 0; i < 200; i++)
{
    var theta = 2.0f * MathF.PI * i / 200.0f;
    var target = center + new Vector3(30.0f * MathF.Cos(theta), 30.0f * MathF.Sin(theta), 0.0f);
    var buffer = geometry.PatternBuffer();
    Pattern.Focus(
        geometry,
        target,
        wavelength,
        new FocusOption(),
        buffer
    );
    patterns[i] = buffer;
}

var builder = client.DatagramBuilder();
builder.Push(new PatternStm(
    1.0f * Hz,
    patterns,
    new PatternStmOption(
        bank: PatternBank.B0,
        mode: PatternStmMode.PhaseIntensityFull,
        loopBehavior: LoopBehavior.Infinite,
        transitionMode: TransitionMode.Immediate
    )
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
