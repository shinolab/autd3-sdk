using System;
using System.Linq;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandFociStmExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = Client.Open(geometry, new TransportOption());
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
var points = Enumerable.Range(0, 200)
    .Select(i => 2.0f * MathF.PI * i / 200)
    .Select(theta => (ControlPoints)(center + Offset(MathF.Cos(theta) * 30.0f * mm, MathF.Sin(theta) * 30.0f * mm, 0.0f * mm)))
    .ToArray();

await client.SendAsync(new FociStm(
    1.0f * Hz,
    points,
    new FociStmOption
    {
        Bank = PatternBank.B0,
        SoundSpeed = 340.0f * m / s,
        LoopBehavior = LoopBehavior.Infinite,
        TransitionMode = TransitionMode.Immediate,
    }
));
        // HIDE
    }
}
// HIDE_END
