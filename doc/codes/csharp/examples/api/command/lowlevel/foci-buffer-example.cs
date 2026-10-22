using System;
using System.Linq;
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
await using var client = Client.Open(geometry, new TransportOption());
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
var points = Enumerable.Range(0, 200)
    .Select(i => 2.0f * MathF.PI * i / 200)
    .Select(theta => (ControlPoints)(center + Offset(MathF.Cos(theta) * 30.0f * mm, MathF.Sin(theta) * 30.0f * mm, 0.0f * mm)))
    .ToArray();

var bank = PatternBank.B0;

var maxLen = WriteFociBuffer.MaxLen(1);
for (var startIndex = 0; startIndex < points.Length; startIndex += maxLen)
{
    await client.SendAsync(new WriteFociBuffer(
        bank: bank,
        startIndex: (uint)startIndex,
        points: points[startIndex..Math.Min(startIndex + maxLen, points.Length)]
    ));
}
await client.SendAsync(new ConfigFociStm(
    bank: bank,
    config: new SamplingConfig(1.0f * Hz * points.Length),
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
