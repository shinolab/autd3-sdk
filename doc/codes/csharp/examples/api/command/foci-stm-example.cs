using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandFociStmExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

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

var builder = client.DatagramBuilder();
builder.Push(new FociStm(
    1.0f * Hz,
    points,
    new FociStmOption(
        bank: PatternBank.B0,
        soundSpeed: 340.0f * m / s,
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
