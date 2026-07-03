using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

// HIDE
namespace AUTD3.DocSamples.ApiCommandFociStmExample;

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

var builder = client.DatagramBuilder();
builder.Push(new FociStm(
    StmConfig.FromFreq(1.0f * Hz),
    points,
    new FociStmOption(
        bank: PatternBank.B0,
        soundSpeedMS: 340.0f,
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
