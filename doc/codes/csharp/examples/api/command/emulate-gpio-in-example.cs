using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandEmulateGpioInExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
await using var client = Client.Open(geometry, new TransportOption());
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

await client.SendAsync(new EmulateGpioIn(new[] { true, false, false, false }));
        // HIDE
    }
}
// HIDE_END
