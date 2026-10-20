using System.Collections.Generic;
using System.Numerics;
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
await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
var dst = new List<ControlPoints>();
Stm.Circle(
    center,
    30.0f * mm,
    200,
    Vector3.UnitZ,
    Intensity.Max,
    dst
);
var points = dst.ToArray();

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
