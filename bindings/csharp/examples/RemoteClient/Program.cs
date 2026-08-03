// Remote Link client: connects to a remote server over TCP and emits a 200 Hz sine AM focus
// Start the Rust remote_server example (or the simulator) first.
// Pass an address to skip the mDNS lookup; without one it falls back to the local default
// when no appliance answers, which is where remote_server and the simulator both listen.
//
// Run with: cargo xtask cs example RemoteClient

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

internal static class Program
{
    private const string LocalAddr = "127.0.0.1:8080";

    private static RemoteLinkOption LinkOption(string[] args)
    {
        if (args.Length > 0)
        {
            return new RemoteLinkOption(args[0]);
        }
        try
        {
            return RemoteLinkOption.Discover();
        }
        catch (Autd3Exception e)
        {
            Console.WriteLine($"discovery found no appliance ({e.Message}); falling back to {LocalAddr}");
            return new RemoteLinkOption(LocalAddr);
        }
    }

    private static async Task Main(string[] args)
    {
        var option = LinkOption(args);
        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });

        using var client = await Client.OpenAsync(geometry, option, new ClientConfig());

        Console.WriteLine($"connected to {option.Addr}, devices: {client.NumDevices}");
        var versions = await client.ReadFirmwareVersionAsync();
        for (var i = 0; i < versions.Count; i++)
        {
            Console.WriteLine($"device[{i}] firmware version: {versions[i]}");
        }

        var target = geometry.Center + new Vector3(0f, 0f, 150f);
        var wavelength = Pattern.Wavelength(340 * m / s);
        using var patterns = geometry.PatternBuffer();
        Pattern.Focus(geometry, target, wavelength, new FocusOption(), patterns);

        using var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(200 * Hz, new SineOption(), modulation);

        using var builder = client.DatagramBuilder();
        builder
            .Push(new SetSilencer())
            .Push(new Pattern(patterns))
            .Push(new Modulation(SamplingConfig.Freq4k, modulation));
        using var frames = builder.Build();
        foreach (var frame in frames)
        {
            await client.SendCheckedAsync(frame);
        }

        Console.WriteLine("emitting a 200 Hz AM focus over the network — press Ctrl+C to stop");

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
