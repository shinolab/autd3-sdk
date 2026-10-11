using AUTD3;

namespace DocSamples.ApiBasicsTracing;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        var option = new TracingOption
        {
            DefaultFilter = "info",
            Writer = LogWriter.Stdout,
        };
        using var logGuard = Tracing.Init(option);
        // ANCHOR_END: api
    }
}
