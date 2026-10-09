// Single focus with a 200 Hz sine AM. Run with: cargo xtask cs example FocusSine

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using static AUTD3.Units;

internal static class Program
{
    private static async Task Main()
    {
        using var logGuard = Tracing.Init(new TracingOption());

        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });

        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");
        var versions = await client.ReadFirmwareVersionAsync();
        for (var i = 0; i < versions.Count; i++)
        {
            Console.WriteLine($"device[{i}] firmware version: {versions[i]}");
        }

        // length in mm
        var target = geometry.Center + new Vector3(0f, 0f, 150f);
        var wavelength = Pattern.Wavelength(340 * m / s);
        using var phases = geometry.PhaseBuffer();
        Pattern.Focus(geometry, target, wavelength, phases);

        using var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(200 * Hz, new SineOption(), modulation);

        await client.SendAsync(new Pattern(phases, Intensity.Max));
        await client.SendAsync(new Modulation(SamplingConfig.Freq4k, modulation));

        Console.WriteLine(
            $"emitting a 200 Hz AM focus at ({target.X:F2}, {target.Y:F2}, {target.Z:F2}) mm — press Ctrl+C to stop");

        var stop = new TaskCompletionSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            stop.TrySetResult();
        };
        await stop.Task;
    }
}
