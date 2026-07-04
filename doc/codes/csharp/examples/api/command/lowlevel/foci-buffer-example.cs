using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using Nop = AUTD3.Link.Nop;
using static AUTD3.Units;

// HIDE
namespace DocSamples.ApiCommandLowlevelFociBufferExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, new Nop(), new ClientConfig());

var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
var points = Stm.Circle(
    center,
    30.0f * mm,
    200,
    Vector3.UnitZ,
    Intensity.Max
);

var bank = PatternBank.B0;

var builder = client.DatagramBuilder();
builder.Push(new WriteFociBuffer(
    bank: bank,
    indexOffset: 0,
    points: points
));
builder.Push(new ConfigFociStm(
    bank: bank,
    config: new SamplingConfig(points.Length * Hz),
    size: (uint)points.Length,
    numFoci: 1,
    soundSpeed: 340.0f * m / s,
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

await client.CloseAsync();
        // HIDE
    }
}
// HIDE_END
