// Pattern STM: a circle of host-computed focus patterns played back at 1 Hz.
// Run with: cargo xtask cs example PatternStm

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;
using static AUTD3.Units;

internal static class Program
{
    private const int NumPoints = 200;
    private const float RadiusMm = 30f;

    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });

        using var client = await Client.OpenAsync(geometry, new EchocatLinkOption(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        var center = geometry.Center + new Vector3(0f, 0f, 150f);
        var wavelength = Pattern.Wavelength(340 * m / s);
        var focusOption = new FocusOption();
        var patterns = new List<PatternBuffer>();
        try
        {
            for (var i = 0; i < NumPoints; i++)
            {
                var theta = 2f * MathF.PI * i / NumPoints;
                var target = center + new Vector3(RadiusMm * MathF.Cos(theta), RadiusMm * MathF.Sin(theta), 0f);
                var buffer = geometry.PatternBuffer();
                Pattern.Focus(geometry, target, wavelength, focusOption, buffer);
                patterns.Add(buffer);
            }

            using var builder = client.DatagramBuilder();
            builder
                .Push(new SetSilencer())
                .Push(new PatternStm(1 * Hz, patterns.ToArray(),
                    new PatternStmOption(mode: PatternStmMode.PhaseFull)));
            using var frames = builder.Build();
            foreach (var frame in frames)
            {
                await client.SendCheckedAsync(frame);
            }

            Console.WriteLine("running a 1 Hz circular pattern STM — press Ctrl+C to stop");

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
        finally
        {
            foreach (var buffer in patterns)
            {
                buffer.Dispose();
            }
        }
    }
}
