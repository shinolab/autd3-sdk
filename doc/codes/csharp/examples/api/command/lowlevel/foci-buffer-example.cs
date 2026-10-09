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
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
using var emulator = new UdpEmulator(geometry.NumDevices);
await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());

var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
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
