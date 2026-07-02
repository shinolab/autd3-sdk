// Single focus with a 200 Hz sine AM. Run with: cargo xtask cs example FocusSine

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

internal static class Program
{
    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Device> { new Device(Vector3.Zero) });

        using var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");
        var versions = await client.ReadFirmwareVersionAsync();
        for (var i = 0; i < versions.Count; i++)
        {
            Console.WriteLine($"device[{i}] firmware version: {versions[i]}");
        }

        // length in mm; sound speed as a Velocity
        var target = geometry.Center + new Vector3(0f, 0f, 150f);
        var wavelength = Pattern.Wavelength(340 * m / s);
        using var patterns = geometry.PatternBuffer();
        Pattern.Focus(geometry, target, wavelength, new FocusOption(), patterns);

        using var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(200 * Hz, new SineOption(), modulation);

        using var builder = client.DatagramBuilder();
        builder
            .Push(new Pattern(patterns))
            .Push(new Modulation(SamplingConfig.Freq4k, modulation));
        using var datagrams = builder.Build();
        foreach (var frame in datagrams)
        {
            await client.SendCheckedAsync(frame);
        }

        Console.WriteLine(
            $"emitting a 200 Hz AM focus at ({target.X:F2}, {target.Y:F2}, {target.Z:F2}) mm — press Ctrl+C to stop");

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
