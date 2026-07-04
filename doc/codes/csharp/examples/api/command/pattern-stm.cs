using System;
using System.Numerics;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiCommandPatternStm;

internal static class Sample
{
    private const int NumPoints = 200;
    private const float RadiusMm = 30.0f;

    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        // Compute the Pattern (emission of all transducers) for each sample point on the host.
        var center = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var patterns = new PatternBuffer[NumPoints];
        for (var i = 0; i < NumPoints; i++)
        {
            var theta = 2.0f * MathF.PI * i / NumPoints;
            var target = center + new Vector3(RadiusMm * MathF.Cos(theta), RadiusMm * MathF.Sin(theta), 0.0f);
            var buffer = geometry.PatternBuffer();
            Pattern.Focus(geometry, target, wavelength, new FocusOption(), buffer);
            patterns[i] = buffer;
        }
        var freq = 1.0f * Hz;
        var option =
            // ANCHOR: option
            new PatternStmOption(
                bank: PatternBank.B0,
                mode: PatternStmMode.PhaseIntensityFull,
                loopBehavior: LoopBehavior.Infinite,
                transitionMode: TransitionMode.Immediate
            )
            // ANCHOR_END: option
            ;
        // ANCHOR: api
        new PatternStm(new StmConfig(freq), patterns, option);
        // ANCHOR_END: api

        // ANCHOR: equivalent
        for (var index = 0; index < patterns.Length; index++)
        {
            new WritePatternBuffer(
                bank: option.Bank,
                index: (ushort)index,
                emissions: patterns[index]
            );
        }
        new ConfigPattern(
            bank: option.Bank,
            config: new SamplingConfig(patterns.Length * Hz),
            size: (uint)patterns.Length,
            loopBehavior: option.LoopBehavior
        );
        new ChangePatternBank(
            bank: option.Bank,
            transitionMode: option.TransitionMode
        );
        // ANCHOR_END: equivalent
    }
}
