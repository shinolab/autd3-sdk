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

    private static readonly (string Name, Func<Telemetry, uint> Read)[] Counters =
    {
        ("FifoDrop", t => t.FifoDrop),
        ("Dedup", t => t.Dedup),
        ("SeqMismatch", t => t.SeqMismatch),
        ("DispatchError", t => t.DispatchError),
        ("Processed", t => t.Processed),
        ("SendFailure", t => t.SendFailure),
        ("Failsafe", t => t.Failsafe),
        ("PtpUnlockFailsafe", t => t.PtpUnlockFailsafe),
        ("SyncResync", t => t.SyncResync),
        ("BootFailure", t => t.BootFailure),
    };

    private static void PrintSnapshot(IReadOnlyList<Telemetry> snapshot)
    {
        foreach (var (name, read) in Counters)
        {
            var values = snapshot.Select(read);
            Console.WriteLine($"{name}: [{string.Join(", ", values)}]");
        }
    }

    private static void PrintDeltas(IReadOnlyList<Telemetry> before, IReadOnlyList<Telemetry> after)
    {
        foreach (var (name, read) in Counters)
        {
            var deltas = before.Zip(after, (b, a) => unchecked(read(a) - read(b))).ToArray();
            if (deltas.Any(delta => delta != 0))
            {
                Console.WriteLine($"{name}: +[{string.Join(", ", deltas)}]");
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

        IReadOnlyList<Telemetry>? baseline = null;
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
