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
        var dst = geometry.PatternBuffer();

        // ANCHOR: set_intensity
        Pattern.SetIntensity(intensity, dst);
        // ANCHOR_END: set_intensity

        // ANCHOR: set_phase
        Pattern.SetPhase(phase, dst);
        // ANCHOR_END: set_phase

        // ANCHOR: set_phase_and_intensity
        Pattern.SetPhaseAndIntensity(phase, intensity, dst);
        // ANCHOR_END: set_phase_and_intensity

        // ANCHOR: add_phase
        Pattern.AddPhase(phase, dst);
        // ANCHOR_END: add_phase
    }
}
