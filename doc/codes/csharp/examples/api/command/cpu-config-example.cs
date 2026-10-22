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
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

await client.SendAsync(new SetCpuConfig(new CpuConfig { SysTimeTransitionMargin = TimeSpan.FromMilliseconds(20) }));
        // HIDE
    }
}
// HIDE_END
