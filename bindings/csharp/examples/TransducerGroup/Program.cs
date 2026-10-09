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
    private enum Side
    {
        Left,
        Right,
    }

    private static async Task Main()
    {
        using var logGuard = Tracing.Init(new TracingOption());

        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });

        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        var wavelength = Pattern.Wavelength(340 * m / s);
        var center = geometry.Center;

        var groups = new TransducerGroups<Side>(geometry, (device, tr) =>
            device.Position(tr).X < center.X ? Side.Left : Side.Right);

        var leftFoci = new[]
        {
            new AmplitudeTarget(center + Offset(-50 * mm, 0 * mm, 150 * mm), 2.5e3f * Pa),
            new AmplitudeTarget(center + Offset(-20 * mm, 0 * mm, 150 * mm), 2.5e3f * Pa),
        };
        var rightTarget = center + Offset(40 * mm, 0 * mm, 150 * mm);

        using var phases = geometry.PhaseBuffer();
        using var intensities = geometry.IntensityBuffer();
        Pattern.GroupCompute(
            geometry,
            groups,
            (side, mask, groupPhases, groupIntensities) =>
            {
                switch (side)
                {
                    case Side.Left:
                        Holo.Gspat(geometry, leftFoci, wavelength, new GspatOption { Mask = mask }, groupPhases, groupIntensities);
                        break;
                    default:
                        Pattern.Focus(geometry, rightTarget, wavelength, groupPhases);
                        break;
                }
            },
            phases,
            intensities);

        await client.SendAsync(new SetSilencer());
        await client.SendAsync(new Pattern(phases, intensities));

        Console.WriteLine("left half -> two GSPAT foci, right half -> single focus — press Ctrl+C to stop");

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
