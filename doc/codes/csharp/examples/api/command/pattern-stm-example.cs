using System;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandPatternStmExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
var wavelength = Pattern.Wavelength(340.0f * m / s);
var patterns = new PhaseBuffer[200];
for (var i = 0; i < 200; i++)
{
    var theta = 2.0f * MathF.PI * i / 200.0f;
    var target = center + Offset(30.0f * MathF.Cos(theta) * mm, 30.0f * MathF.Sin(theta) * mm, 0.0f * mm);
    var buffer = geometry.PhaseBuffer();
    Pattern.Focus(
        geometry,
        target,
        wavelength,
        buffer
    );
    patterns[i] = buffer;
}

await client.SendAsync(new PatternStm(
    1.0f * Hz,
    patterns,
    Intensity.Max,
    new PatternStmOption
    {
        Bank = PatternBank.B0,
        PhaseDepth = PhaseDepth.Bits8,
        LoopBehavior = LoopBehavior.Infinite,
        TransitionMode = TransitionMode.Immediate,
    }
));
        // HIDE
    }
}
// HIDE_END
