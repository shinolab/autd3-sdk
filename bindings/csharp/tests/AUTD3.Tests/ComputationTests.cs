using System;
using System.Numerics;
using AUTD3;
using Xunit;
using static AUTD3.Units;

namespace AUTD3.Tests
{
    public class ComputationTests
    {
        [Fact]
        public void GeometryReportsDeviceCount()
        {
            using var geometry = new Geometry(new[]
            {
                new Autd3(Vector3.Zero),
                new Autd3(new Vector3(192f, 0f, 0f)),
            });
            Assert.Equal(2, geometry.NumDevices);
        }

        [Fact]
        public void WavelengthMatchesSoundSpeed()
        {
            var wavelength = Pattern.Wavelength(340 * m / s);
            Assert.InRange(wavelength.Mm, 8.4f, 8.6f);
        }

        [Fact]
        public void FocusFillsBufferForEveryDevice()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            var wavelength = Pattern.Wavelength(340 * m / s);
            Pattern.SetIntensity(new Intensity(0x80), intensities);
            Pattern.Focus(geometry, geometry.Center + new Vector3(0f, 0f, 150f), wavelength, phases);
            Assert.Equal(1, phases.NumDevices);
            Assert.Equal(1, intensities.NumDevices);
            foreach (var i in intensities[0])
            {
                Assert.Equal(new Intensity(0x80), i);
            }
            Assert.Contains(phases[0], p => p.Value != phases[0][0].Value);
        }

        private enum Side
        {
            Left,
            Right,
        }

        private static TransducerGroups<Side> Sides(Geometry geometry) =>
            new TransducerGroups<Side>(geometry, (device, tr) => Expected(device.Idx, tr, Side.Left, Side.Right));

        private static T Expected<T>(int dev, int tr, T left, T right) => (dev, tr % 3) switch
        {
            (1, 1) => right,
            _ => left,
        };

        [Fact]
        public void GroupCopiesTheSourceOfEachKey()
        {
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero), new Autd3(new Vector3(200f, 0f, 0f)) });
            using var left = geometry.PhaseBuffer();
            Pattern.SetPhase(new Phase(0x10), left);
            using var right = geometry.PhaseBuffer();
            Pattern.SetPhase(new Phase(0x30), right);
            using var dst = geometry.PhaseBuffer();
            Pattern.SetPhase(new Phase(0xFF), dst);
            using var leftI = geometry.IntensityBuffer();
            Pattern.SetIntensity(new Intensity(0x20), leftI);
            using var rightI = geometry.IntensityBuffer();
            Pattern.SetIntensity(new Intensity(0x40), rightI);
            using var dstI = geometry.IntensityBuffer();
            Pattern.SetIntensity(new Intensity(0x60), dstI);

            var groups = Sides(geometry);
            Assert.Equal(new[] { Side.Left, Side.Right }, groups.Keys);
            Assert.Equal(Side.Right, groups.Key(1, 1));
            Assert.Equal(Side.Left, groups.Key(0, 1));

            Pattern.Group(geometry, groups, side => side == Side.Left ? left : right, dst);
            Pattern.Group(geometry, groups, side => side == Side.Left ? leftI : rightI, dstI);

            for (var dev = 0; dev < 2; dev++)
            {
                for (var tr = 0; tr < dst[dev].NumTransducers; tr++)
                {
                    Assert.Equal(Expected(dev, tr, new Phase(0x10), new Phase(0x30)), dst[dev][tr]);
                    Assert.Equal(Expected(dev, tr, new Intensity(0x20), new Intensity(0x40)), dstI[dev][tr]);
                }
            }

            Assert.Throws<Autd3Exception>(() => Pattern.Group(geometry, groups, side => side == Side.Left ? left : dst, dst));
            Assert.Throws<Autd3Exception>(() => Pattern.Group(geometry, groups, side => side == Side.Left ? left : null!, dst));

            using var single = Fixture.SingleDevice();
            using var singleBuffer = single.PhaseBuffer();
            Assert.Throws<Autd3Exception>(() => Pattern.Group(geometry, groups, side => side == Side.Left ? left : singleBuffer, dst));
            Assert.Throws<Autd3Exception>(() => Pattern.Group(single, groups, _ => singleBuffer, dst));
            Assert.Null(new TransducerGroups<Side>(single, (_, _) => Side.Left).Mask(Side.Right));
        }

        [Fact]
        public void GroupMaskRestrictsHoloToTheGroup()
        {
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero), new Autd3(new Vector3(200f, 0f, 0f)) });
            var groups = new TransducerGroups<Side>(geometry, (device, tr) => device.Idx == 1 && tr % 3 == 1 ? Side.Left : Side.Right);
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            var foci = new[] { new AUTD3.Holo.AmplitudeTarget(geometry.Center + new Vector3(0f, 0f, 150f), 5e3f * AUTD3.Holo.HoloUnits.Pa) };

            AUTD3.Holo.Holo.Naive(geometry, foci, Pattern.Wavelength(340 * m / s), new AUTD3.Holo.NaiveOption { Constraint = AUTD3.Holo.IntensityConstraint.Uniform(Intensity.Max), Mask = groups.Mask(Side.Left)!.Value }, phases, intensities);

            for (var dev = 0; dev < 2; dev++)
            {
                var transducers = intensities[dev];
                for (var tr = 0; tr < transducers.NumTransducers; tr++)
                {
                    var expected = dev == 1 && tr % 3 == 1 ? Intensity.Max : Intensity.Min;
                    Assert.Equal(expected, transducers[tr]);
                }
            }

            using var single = Fixture.SingleDevice();
            using var singlePhases = single.PhaseBuffer();
            using var singleIntensities = single.IntensityBuffer();
            Assert.Throws<Autd3Exception>(() => AUTD3.Holo.Holo.Naive(single, foci, Pattern.Wavelength(340 * m / s), new AUTD3.Holo.NaiveOption { Constraint = AUTD3.Holo.IntensityConstraint.Uniform(Intensity.Max), Mask = groups.Mask(Side.Left)!.Value }, singlePhases, singleIntensities));
        }

        [Fact]
        public void GroupComputePassesTheMaskOfEachKey()
        {
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero), new Autd3(new Vector3(200f, 0f, 0f)) });
            var groups = Sides(geometry);
            using var phases = geometry.PhaseBuffer();
            Pattern.SetPhase(new Phase(0xFF), phases);
            using var intensities = geometry.IntensityBuffer();
            Pattern.SetIntensity(new Intensity(0x60), intensities);
            var foci = new[] { new AUTD3.Holo.AmplitudeTarget(geometry.Center + new Vector3(0f, 0f, 150f), 5e3f * AUTD3.Holo.HoloUnits.Pa) };
            var seen = new System.Collections.Generic.List<Side>();

            Pattern.GroupCompute(geometry, groups, (side, mask, p, i) =>
            {
                seen.Add(side);
                if (side == Side.Left)
                {
                    AUTD3.Holo.Holo.Naive(geometry, foci, Pattern.Wavelength(340 * m / s), new AUTD3.Holo.NaiveOption { Constraint = AUTD3.Holo.IntensityConstraint.Uniform(Intensity.Max), Mask = mask }, p, i);
                }
                else
                {
                    Pattern.SetPhase(new Phase(0x30), p);
                    Pattern.SetIntensity(new Intensity(0x40), i);
                }
            }, phases, intensities);

            Assert.Equal(new[] { Side.Left, Side.Right }, seen);
            for (var dev = 0; dev < 2; dev++)
            {
                for (var tr = 0; tr < phases[dev].NumTransducers; tr++)
                {
                    if (Expected(dev, tr, true, false))
                    {
                        Assert.Equal(Intensity.Max, intensities[dev][tr]);
                    }
                    else
                    {
                        Assert.Equal(new Phase(0x30), phases[dev][tr]);
                        Assert.Equal(new Intensity(0x40), intensities[dev][tr]);
                    }
                }
            }

            using var single = Fixture.SingleDevice();
            Assert.Throws<Autd3Exception>(() => Pattern.GroupCompute(single, groups, (_, _, _, _) => { }, phases, intensities));

            var calls = 0;
            using var singlePhases = single.PhaseBuffer();
            using var singleIntensities = single.IntensityBuffer();
            Assert.Throws<Autd3Exception>(() => Pattern.GroupCompute(geometry, groups, (_, _, _, _) => { calls++; }, singlePhases, intensities));
            Assert.Throws<Autd3Exception>(() => Pattern.GroupCompute(geometry, groups, (_, _, _, _) => { calls++; }, phases, singleIntensities));
            Assert.Equal(0, calls);
            Assert.Throws<ArgumentNullException>(() => Pattern.GroupCompute(geometry, groups, null!, phases, intensities));
            Assert.Throws<InvalidOperationException>(() => Pattern.GroupCompute(geometry, groups, (_, _, _, _) => throw new InvalidOperationException(), phases, intensities));

            Pattern.GroupCompute(geometry, groups, (_, _, _, _) => { }, phases, intensities);
            for (var dev = 0; dev < 2; dev++)
            {
                for (var tr = 0; tr < phases[dev].NumTransducers; tr++)
                {
                    Assert.Equal(Phase.Zero, phases[dev][tr]);
                    Assert.Equal(Intensity.Max, intensities[dev][tr]);
                }
            }
        }

        [Fact]
        public void SineProducesSamples()
        {
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption { SamplingConfig = SamplingConfig.Freq4k }, modulation);
            Assert.True(modulation.Length > 0);
        }

        [Fact]
        public void ConstantFillsTwoSamples()
        {
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Constant(0xFF, modulation);
            Assert.Equal(2, modulation.Length);
        }

        [Fact]
        public void SamplingConfigResolvesDivider()
        {
            Assert.True(SamplingConfig.Freq4k.Divide() > 0);
        }

        [Fact]
        public void BuildDatagramsFromCommands()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            Pattern.Focus(geometry, geometry.Center + new Vector3(0f, 0f, 150f), Pattern.Wavelength(340 * m / s), phases);
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption(), modulation);

            using var frames = Frames.Encode(geometry, Command.Sequence(
                new Pattern(phases, intensities),
                new Modulation(SamplingConfig.Freq4k, modulation)));

            Assert.True(frames.Length > 0);

            var frameCount = 0;
            foreach (var frame in frames)
            {
                _ = frame;
                frameCount++;
            }
            Assert.Equal(frames.Length, frameCount);
        }

        [Fact]
        public void BuildDatagramsFromLowLevelOps()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            Pattern.SetIntensity(Intensity.Min, intensities);

            using var frames = Frames.Encode(geometry, Command.Sequence(
                new WritePatternBuffer(PatternBank.B0, 0, phases, intensities),
                new ConfigPattern(PatternBank.B0, SamplingConfig.Freq4k, 1)));

            Assert.Equal(2, frames.Length);
            _ = frames[0];
            Assert.Throws<ArgumentOutOfRangeException>(() => frames[frames.Length]);
        }
    }
}
