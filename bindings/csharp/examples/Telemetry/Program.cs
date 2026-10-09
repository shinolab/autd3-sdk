using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;

internal static class Program
{
    private static readonly TimeSpan PollInterval = TimeSpan.FromSeconds(1);

    private static void PrintSnapshot(IReadOnlyList<TelemetryCounters> snapshot)
    {
        foreach (var counter in TelemetryExt.All)
        {
            var values = snapshot.Select(c => c.Get(counter));
            Console.WriteLine($"{counter}: [{string.Join(", ", values)}]");
        }
    }

    private static void PrintDeltas(IReadOnlyList<TelemetryCounters> before, IReadOnlyList<TelemetryCounters> after)
    {
        foreach (var counter in TelemetryExt.All)
        {
            var deltas = before.Zip(after, (b, a) => unchecked(a.Get(counter) - b.Get(counter))).ToArray();
            if (deltas.Any(delta => delta != 0))
            {
                Console.WriteLine($"{counter}: +[{string.Join(", ", deltas)}]");
            }
        }
    }

    private static async Task Main()
    {
        using var logGuard = Tracing.Init(new TracingOption());

        using var geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.Zero) });
        await using var client = await Client.OpenAsync(geometry, new TransportOption(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");

        using var cts = new CancellationTokenSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            cts.Cancel();
        };

        IReadOnlyList<TelemetryCounters>? baseline = null;
        Console.WriteLine("polling telemetry — press Ctrl+C to stop");
        while (!cts.IsCancellationRequested)
        {
            var snapshot = await client.ReadTelemetryAsync();
            if (baseline is null)
            {
                PrintSnapshot(snapshot);
            }
            else
            {
                PrintDeltas(baseline, snapshot);
            }
            baseline = snapshot;

            try
            {
                await Task.Delay(PollInterval, cts.Token);
            }
            catch (TaskCanceledException)
            {
            }
        }
    }
}
