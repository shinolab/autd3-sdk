using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandLowlevelFociBufferExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = await Client.OpenAsync(geometry, new UdpOption(), new ClientConfig());

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

var bank = PatternBank.B0;

await client.SendAsync(new WriteFociBuffer(
    bank: bank,
    indexOffset: 0,
    points: points
));
await client.SendAsync(new ConfigFociStm(
    bank: bank,
    config: new StmConfig(1.0f * Hz).IntoSamplingConfig(points.Length),
    size: (uint)points.Length,
    numFoci: 1,
    soundSpeed: 340.0f * m / s,
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
