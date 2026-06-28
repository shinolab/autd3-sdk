// Two simultaneous foci via the GS-PAT holographic algorithm, with a 200 Hz sine AM.
// Run with: cargo xtask cs example Holo

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

internal static class Program
{
    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Device> { new Device(Vector3.Zero) });

        using var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        var center = geometry.Center + new Vector3(0f, 0f, 150f);
        var wavelength = Pattern.Wavelength(340f * 1000f);
        var foci = new[]
        {
            new HoloControlPoint(center + new Vector3(-20f, 0f, 0f), Amplitude.FromSpl(150f)),
            new HoloControlPoint(center + new Vector3(20f, 0f, 0f), Amplitude.FromSpl(150f)),
        };

        using var patterns = geometry.PatternBuffer();
        Holo.Gspat(geometry, foci, wavelength, new GspatOption(constraint: EmissionConstraint.Uniform(Intensity.Max)), patterns);

        using var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(200f, new SineOption(), modulation);

        using var builder = client.DatagramBuilder();
        builder
            .Push(new Pattern(patterns))
            .Push(new Modulation(SamplingConfig.Freq4k, modulation));
        using var datagrams = builder.Build();
        foreach (var frame in datagrams)
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
