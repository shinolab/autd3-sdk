using AUTD3;

namespace DocSamples.ApiCommandPhaseCorrection;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        using var phases = geometry.PhaseBuffer();

        // ANCHOR: api
        new SetPhaseCorrection(phases);
        // ANCHOR_END: api
    }
}
