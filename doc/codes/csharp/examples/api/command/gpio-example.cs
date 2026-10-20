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
await using var client = await Client.OpenAsync(geometry, new UdpOption(), new ClientConfig());

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
