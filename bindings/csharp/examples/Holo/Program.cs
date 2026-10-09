// Two simultaneous foci via the GS-PAT holographic algorithm, with a 200 Hz sine AM.
// Run with: cargo xtask cs example Holo

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Holo;
using static AUTD3.Holo.HoloUnits;
using static AUTD3.Units;

internal static class Program
{
    private static async Task Main()
    {
        using var logGuard = Tracing.Init(new TracingOption());

        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });

        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        var center = geometry.Center + Offset(0 * mm, 0 * mm, 150 * mm);
        var foci = new[]
        {
            new AmplitudeTarget(center + Offset(-30 * mm, 0 * mm, 0 * mm), 2.5e3f * Pa),
            new AmplitudeTarget(center + Offset(30 * mm, 0 * mm, 0 * mm), 2.5e3f * Pa),
        };

        var wavelength = Pattern.Wavelength(340 * m / s);

        using var phases = geometry.PhaseBuffer();
        using var intensities = geometry.IntensityBuffer();
        Holo.Gspat(geometry, foci, wavelength, new GspatOption(), phases, intensities);

        using var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(200 * Hz, new SineOption(), modulation);

        await client.SendAsync(new SetSilencer());
        await client.SendAsync(new Pattern(phases, intensities));
        await client.SendAsync(new Modulation(SamplingConfig.Freq4k, modulation));

        Console.WriteLine("emitting two GS-PAT foci with a 200 Hz AM — press Ctrl+C to stop");

        var stop = new TaskCompletionSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            stop.TrySetResult();
        };
        await stop.Task;

        await client.SilentStopAsync();
    }
}
