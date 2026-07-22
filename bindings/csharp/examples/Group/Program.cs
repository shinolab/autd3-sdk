// Per-device-group command: focus each device group at a different target.
// Run with: cargo xtask cs example Group

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
        using var geometry = new Geometry(new List<Autd3>
        {
            new Autd3(Vector3.Zero),
            new Autd3(new Vector3(Autd3.DeviceWidth, 0f, 0f)),
        });

        using var client = await Client.OpenAsync(geometry, new EtherCrabLinkOption(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        var wavelength = Pattern.Wavelength(340 * m / s);
        var focusOption = new FocusOption();

        var leftTarget = geometry.Center + new Vector3(-40f, 0f, 150f);
        using var left = geometry.PatternBuffer();
        Pattern.Focus(geometry, leftTarget, wavelength, focusOption, left);

        var rightTarget = geometry.Center + new Vector3(40f, 0f, 150f);
        using var right = geometry.PatternBuffer();
        Pattern.Focus(geometry, rightTarget, wavelength, focusOption, right);

        using var builder = client.DatagramBuilder();
        builder
            .Push(new SetSilencer())
            .PushEach(device => new Pattern(device.Idx % 2 == 0 ? left : right));
        using var frames = builder.Build();
        foreach (var frame in frames)
        {
            await client.SendCheckedAsync(frame);
        }

        Console.WriteLine("even devices -> left target, odd devices -> right target — press Ctrl+C to stop");

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
