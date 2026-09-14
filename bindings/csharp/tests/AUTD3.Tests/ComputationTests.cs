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
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
            using var buffer = geometry.PatternBuffer();
            var wavelength = Pattern.Wavelength(340 * m / s);
            Pattern.Focus(geometry, geometry.Center + new Vector3(0f, 0f, 150f), wavelength, Intensity.Max, buffer);
            Assert.Equal(1, buffer.NumDevices);
        }

        private enum Side
        {
            Left,
            Right,
        }

        [Fact]
        public void GroupCopiesTheSourceOfEachKey()
        {
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero), new Autd3(new Vector3(200f, 0f, 0f)) });
            using var left = geometry.PatternBuffer();
            Pattern.Uniform(new Emission(new Phase(0x10), new Intensity(0x20)), left);
            using var right = geometry.PatternBuffer();
            Pattern.Uniform(new Emission(new Phase(0x30), new Intensity(0x40)), right);
            using var dst = geometry.PatternBuffer();

            var groups = new TransducerGroups<Side>(geometry, (device, tr) => (device.Idx, tr % 3) switch
            {
                (_, 0) => Side.Left,
                (1, 1) => Side.Right,
                _ => (Side?)null,
            });
            Assert.Equal(new[] { Side.Left, Side.Right }, groups.Keys);
            Assert.Equal((Side?)Side.Right, groups.Key(1, 1));
            Assert.Null(groups.Key(0, 1));

            Pattern.Group(geometry, groups, side => side == Side.Left ? left : right, dst);

            var leftEmission = new Emission(new Phase(0x10), new Intensity(0x20));
            var rightEmission = new Emission(new Phase(0x30), new Intensity(0x40));
            for (var dev = 0; dev < 2; dev++)
            {
                var transducers = dst[dev];
                for (var tr = 0; tr < transducers.NumTransducers; tr++)
                {
                    var expected = (dev, tr % 3) switch
                    {
                        (_, 0) => leftEmission,
                        (1, 1) => rightEmission,
                        _ => Emission.Null,
                    };
                    Assert.Equal(expected, transducers[tr]);
                }
            }

            Assert.Throws<Autd3Exception>(() => Pattern.Group(geometry, groups, side => side == Side.Left ? left : dst, dst));
            Assert.Throws<Autd3Exception>(() => Pattern.Group(geometry, groups, side => side == Side.Left ? left : null!, dst));

            using var single = new Geometry(new[] { new Autd3(Vector3.Zero) });
            using var singleBuffer = single.PatternBuffer();
            Assert.Throws<Autd3Exception>(() => Pattern.Group(geometry, groups, side => side == Side.Left ? left : singleBuffer, dst));
            Assert.Throws<Autd3Exception>(() => Pattern.Group(single, groups, _ => singleBuffer, dst));
            Assert.Throws<ArgumentException>(() => new TransducerGroups<Side>(single, (_, _) => Side.Left).Mask(Side.Right));
        }

        [Fact]
        public void GroupMaskRestrictsHoloToTheGroup()
        {
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero), new Autd3(new Vector3(200f, 0f, 0f)) });
            var groups = new TransducerGroups<Side>(geometry, (device, tr) => device.Idx == 1 && tr % 3 == 1 ? Side.Left : (Side?)null);
            using var buffer = geometry.PatternBuffer();
            var foci = new[] { new AUTD3.Holo.AmplitudeTarget(geometry.Center + new Vector3(0f, 0f, 150f), 5e3f * AUTD3.Holo.HoloUnits.Pa) };

            AUTD3.Holo.Holo.Naive(geometry, foci, Pattern.Wavelength(340 * m / s), new AUTD3.Holo.NaiveOption(AUTD3.Holo.EmissionConstraint.Uniform(Intensity.Max), mask: groups.Mask(Side.Left)), buffer);

            for (var dev = 0; dev < 2; dev++)
            {
                var transducers = buffer[dev];
                for (var tr = 0; tr < transducers.NumTransducers; tr++)
                {
                    var expected = dev == 1 && tr % 3 == 1 ? Intensity.Max : Intensity.Min;
                    Assert.Equal(expected, transducers[tr].Intensity);
                }
            }

            using var single = new Geometry(new[] { new Autd3(Vector3.Zero) });
            using var singleBuffer = single.PatternBuffer();
            Assert.Throws<Autd3Exception>(() => AUTD3.Holo.Holo.Naive(single, foci, Pattern.Wavelength(340 * m / s), new AUTD3.Holo.NaiveOption(AUTD3.Holo.EmissionConstraint.Uniform(Intensity.Max), mask: groups.Mask(Side.Left)), singleBuffer));
        }

        [Fact]
        public void GroupComputePassesTheMaskOfEachKey()
        {
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero), new Autd3(new Vector3(200f, 0f, 0f)) });
            var groups = new TransducerGroups<Side>(geometry, (device, tr) => (device.Idx, tr % 3) switch
            {
                (_, 0) => Side.Left,
                (1, 1) => Side.Right,
                _ => (Side?)null,
            });
            using var dst = geometry.PatternBuffer();
            Pattern.Uniform(new Emission(new Phase(0xFF), new Intensity(0xFF)), dst);
            var foci = new[] { new AUTD3.Holo.AmplitudeTarget(geometry.Center + new Vector3(0f, 0f, 150f), 5e3f * AUTD3.Holo.HoloUnits.Pa) };
            var rightEmission = new Emission(new Phase(0x30), new Intensity(0x40));
            var seen = new System.Collections.Generic.List<Side>();

            Pattern.GroupCompute(geometry, groups, (side, mask, buffer) =>
            {
                seen.Add(side);
                if (side == Side.Left)
                {
                    AUTD3.Holo.Holo.Naive(geometry, foci, Pattern.Wavelength(340 * m / s), new AUTD3.Holo.NaiveOption(AUTD3.Holo.EmissionConstraint.Uniform(Intensity.Max), mask: mask), buffer);
                }
                else
                {
                    Pattern.Uniform(rightEmission, buffer);
                }
            }, dst);

            Assert.Equal(new[] { Side.Left, Side.Right }, seen);
            for (var dev = 0; dev < 2; dev++)
            {
                var transducers = dst[dev];
                for (var tr = 0; tr < transducers.NumTransducers; tr++)
                {
                    switch (dev, tr % 3)
                    {
                        case (_, 0):
                            Assert.Equal(Intensity.Max, transducers[tr].Intensity);
                            break;
                        case (1, 1):
                            Assert.Equal(rightEmission, transducers[tr]);
                            break;
                        default:
                            Assert.Equal(Emission.Null, transducers[tr]);
                            break;
                    }
                }
            }

            using var single = new Geometry(new[] { new Autd3(Vector3.Zero) });
            Assert.Throws<Autd3Exception>(() => Pattern.GroupCompute(single, groups, (_, _, _) => { }, dst));

            var calls = 0;
            using var singleBuffer = single.PatternBuffer();
            Assert.Throws<Autd3Exception>(() => Pattern.GroupCompute(geometry, groups, (_, _, _) => calls++, singleBuffer));
            Assert.Equal(0, calls);
            Assert.Throws<ArgumentNullException>(() => Pattern.GroupCompute(geometry, groups, null!, dst));
            Assert.Throws<InvalidOperationException>(() => Pattern.GroupCompute(geometry, groups, (_, _, _) => throw new InvalidOperationException(), dst));

            Pattern.GroupCompute(geometry, groups, (_, _, _) => { }, dst);
            for (var dev = 0; dev < 2; dev++)
            {
                var transducers = dst[dev];
                for (var tr = 0; tr < transducers.NumTransducers; tr++)
                {
                    Assert.Equal(Emission.Null, transducers[tr]);
                }
            }
        }

        [Fact]
        public void SineProducesSamples()
        {
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption(samplingConfig: SamplingConfig.Freq4k), modulation);
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
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
            using var patterns = geometry.PatternBuffer();
            Pattern.Focus(geometry, geometry.Center + new Vector3(0f, 0f, 150f), Pattern.Wavelength(340 * m / s), Intensity.Max, patterns);
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption(), modulation);

            using var builder = new DatagramBuilder(geometry);
            builder
                .Push(new Pattern(patterns))
                .Push(new Modulation(SamplingConfig.Freq4k, modulation));
            using var frames = builder.Build();

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
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
            using var patterns = geometry.PatternBuffer();
            Pattern.Null(patterns);

            using var builder = new DatagramBuilder(geometry);
            builder
                .Push(new WritePatternBuffer(PatternBank.B0, 0, patterns))
                .Push(new ConfigPattern(PatternBank.B0, SamplingConfig.Freq4k, 1));
            using var frames = builder.Build();

            Assert.Equal(2, frames.Length);
            _ = frames[0];
            Assert.Throws<ArgumentOutOfRangeException>(() => frames[frames.Length]);
        }
    }
}
