using System;
using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandCpuConfigExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
using var client = Client.Open(geometry, new TransportOption());
await client.SendAsync(new Clear());
await client.SendAsync(new Synchronize());

await client.SendAsync(new SetCpuConfig(new CpuConfig { PtpUnlockFailsafeTimeout = TimeSpan.FromSeconds(2) }));
        // HIDE
    }
}
// HIDE_END
