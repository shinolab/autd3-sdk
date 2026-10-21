using System;
using AUTD3;
using static AUTD3.Units;

namespace DocSamples.ApiCommandPatternStm;

internal static class Sample
{
    private const int NumPoints = 200;
    private const float RadiusMm = 30.0f;

    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var center = geometry.Center + Offset(0.0f * mm, 0.0f * mm, 150.0f * mm);
        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var patterns = new PhaseBuffer[NumPoints];
        var intensities = Intensity.Max;
        for (var i = 0; i < NumPoints; i++)
        {
            var theta = 2.0f * MathF.PI * i / NumPoints;
            var target = center + Offset(RadiusMm * MathF.Cos(theta) * mm, RadiusMm * MathF.Sin(theta) * mm, 0.0f * mm);
            var buffer = geometry.PhaseBuffer();
            Pattern.Focus(geometry, target, wavelength, buffer);
            patterns[i] = buffer;
        }
        var config = new SamplingConfig(1.0f * Hz * NumPoints);
        var bank = PatternBank.B0;
        var phaseDepth = PhaseDepth.Bits8;
        var loopBehavior = LoopBehavior.Infinite;
        var transitionMode = TransitionMode.Immediate;
        var option =
            // ANCHOR: option
            new PatternStmOption
            {
                Bank = bank,
                PhaseDepth = phaseDepth,
                LoopBehavior = loopBehavior,
                TransitionMode = transitionMode,
            }
            // ANCHOR_END: option
            ;
        // ANCHOR: api
        new PatternStm(config, patterns, intensities, option);
        // ANCHOR_END: api

        // ANCHOR: equivalent
        var maxCount = option.PhaseDepth.MaxCount();
        for (var startIndex = 0; startIndex < patterns.Length; startIndex += maxCount)
        {
            new WritePatternPhase(
                bank: option.Bank,
                startIndex: (ushort)startIndex,
                depth: option.PhaseDepth,
                intensity: intensities,
                patterns: patterns[startIndex..Math.Min(startIndex + maxCount, patterns.Length)]
            );
        }
        new ConfigPattern(
            bank: option.Bank,
            config: config,
            size: (uint)patterns.Length,
            loopBehavior: option.LoopBehavior
        );
        new ActivatePatternBank(
            bank: option.Bank,
            transitionMode: option.TransitionMode
        );
        // ANCHOR_END: equivalent
    }
}
