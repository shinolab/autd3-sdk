using System.Numerics;
using AUTD3;

namespace DocSamples.ApiComputePatternSet;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var intensity = new Intensity(0x80);
        var phase = Phase.Pi;
        var phases = geometry.PhaseBuffer();
        var intensities = geometry.IntensityBuffer();

        // ANCHOR: set_intensity
        Pattern.SetIntensity(intensity, intensities);
        // ANCHOR_END: set_intensity

        // ANCHOR: set_phase
        Pattern.SetPhase(phase, phases);
        // ANCHOR_END: set_phase

        // ANCHOR: add_phase
        Pattern.AddPhase(phase, phases);
        // ANCHOR_END: add_phase
    }
}
