using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandGpioExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
using var client = Client.Open(geometry, new TransportOption());
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

await client.SendAsync(new SetGpioOut(new[]
{
    GpioOut.PatternBank,
    GpioOut.Thermo,
    GpioOut.PwmOut(0),
    GpioOut.Off,
}));
        // HIDE
    }
}
// HIDE_END
