using System.Linq;
using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandReleaseFailsafeExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
using var client = Client.Open(geometry, new TransportOption());
await client.SendAsync(new Clear());
await client.SendAsync(new Synchronize());

var status = client.DeviceStatus();
if (status.Any(s => s.FailsafeActive))
{
    await client.SendAsync(new ReleaseFailsafe());
}
        // HIDE
    }
}
// HIDE_END
