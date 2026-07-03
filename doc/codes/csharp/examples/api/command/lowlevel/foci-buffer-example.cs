using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.ApiCommandLowlevelFociBufferExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Device(Vector3.Zero) });
var client = await Client.OpenAsync(geometry, NopLink.Create(), new ClientConfig());

var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
var points = Stm.Circle(
    center,
    30.0f,
    200,
    Vector3.UnitZ,
    Intensity.Max
);

var bank = PatternBank.B0;

var builder = client.DatagramBuilder();
builder.Push(new WriteFociBuffer(
    bank,
    0,
    points
));
builder.Push(new ConfigPattern(
    bank,
    SamplingConfig.FromFreq(points.Length * Hz),
    (uint)points.Length,
    PatternDataType.Foci(1, 340),
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
