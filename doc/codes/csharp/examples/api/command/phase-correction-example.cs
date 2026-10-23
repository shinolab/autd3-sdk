using System.Threading.Tasks;
using AUTD3;

// HIDE
namespace DocSamples.ApiCommandPhaseCorrectionExample;

internal static class Sample
{
    internal static async Task Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3() });
using var client = Client.Open(geometry, new TransportOption());
await client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

using var phases = geometry.PhaseBuffer();

await client.SendAsync(new SetPhaseCorrection(phases));
        // HIDE
    }
}
// HIDE_END
