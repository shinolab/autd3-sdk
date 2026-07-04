// Two simultaneous foci via the GS-PAT holographic algorithm, with a 200 Hz sine AM.
// Run with: cargo xtask cs example Holo

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Holo;
using AUTD3.Link;
using ControlPoint = AUTD3.Holo.ControlPoint;
using static AUTD3.Holo.HoloUnits;
using static AUTD3.Units;

internal static class Program
{
    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });

        using var client = await Client.OpenAsync(geometry, new EtherCrabLinkOption(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        var center = geometry.Center;
        var wavelength = Pattern.Wavelength(340 * m / s);
        var foci = new[]
        {
            new ControlPoint(center + new Vector3(-20f, 0f, 150f), 150 * dB),
            new ControlPoint(center + new Vector3(20f, 0f, 150f), 150 * dB),
        };

        using var patterns = geometry.PatternBuffer();
        Holo.Gspat(geometry, foci, wavelength, new GspatOption(repeat: 100), patterns);

        using var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(200 * Hz, new SineOption(), modulation);

        using var builder = client.DatagramBuilder();
        builder
            .Push(new Pattern(patterns))
            .Push(new Modulation(SamplingConfig.Freq4k, modulation));
        using var frames = builder.Build();
        foreach (var frame in frames)
        {
            await client.SendCheckedAsync(frame);
        }

        Console.WriteLine("emitting two GS-PAT foci with a 200 Hz AM — press Ctrl+C to stop");

        var stop = new TaskCompletionSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            stop.TrySetResult();
        };
        await stop.Task;

        await client.StopAsync();
        await client.CloseAsync();
    }
}
