using System;
using System.Numerics;
using AUTD3;
using Xunit;

namespace AUTD3.Tests
{
    public class ComputationTests
    {
        [Fact]
        public void GeometryReportsDeviceCount()
        {
            using var geometry = new Geometry(new[]
            {
                new Device(Vector3.Zero),
                new Device(new Vector3(192f, 0f, 0f)),
            });
            Assert.Equal(2, geometry.NumDevices);
        }

        [Fact]
        public void WavelengthMatchesSoundSpeed()
        {
            var wavelength = Pattern.Wavelength(340f * 1000f);
            Assert.InRange(wavelength, 8.4f, 8.6f);
        }

        [Fact]
        public void FocusFillsBufferForEveryDevice()
        {
            using var geometry = new Geometry(new[] { new Device(Vector3.Zero) });
            using var buffer = geometry.PatternBuffer();
            var wavelength = Pattern.Wavelength(340f * 1000f);
            Pattern.Focus(geometry, geometry.Center + new Vector3(0f, 0f, 150f), wavelength, Intensity.Max, buffer);
            Assert.Equal(1, buffer.NumDevices);
        }

        [Fact]
        public void SineProducesSamples()
        {
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200f, new SineOption(samplingConfig: SamplingConfig.Freq4k), modulation);
            Assert.True(modulation.Length > 0);
        }

        [Fact]
        public void SamplingConfigResolvesDivider()
        {
            Assert.True(SamplingConfig.Freq4k.DivideValue() > 0);
        }

        [Fact]
        public void BuildDatagramsFromCommands()
        {
            using var geometry = new Geometry(new[] { new Device(Vector3.Zero) });
            using var patterns = geometry.PatternBuffer();
            Pattern.Focus(geometry, geometry.Center + new Vector3(0f, 0f, 150f), Pattern.Wavelength(340f * 1000f), Intensity.Max, patterns);
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200f, new SineOption(), modulation);

            using var builder = new DatagramBuilder(geometry);
            builder
                .Push(new Pattern(patterns))
                .Push(new Modulation(SamplingConfig.Freq4k, modulation));
            using var datagrams = builder.Build();

            Assert.True(datagrams.NumFrames > 0);

            var frameCount = 0;
            foreach (var frame in datagrams)
            {
                _ = frame;
                frameCount++;
            }
            Assert.Equal(datagrams.NumFrames, frameCount);
        }

        [Fact]
        public void BuildDatagramsFromLowLevelOps()
        {
            using var geometry = new Geometry(new[] { new Device(Vector3.Zero) });
            using var patterns = geometry.PatternBuffer();
            Pattern.Null(patterns);

            using var builder = new DatagramBuilder(geometry);
            builder
                .Push(new WritePatternBuffer(PatternBank.B0, 0, patterns))
                .Push(new ConfigPattern(PatternBank.B0, SamplingConfig.Freq4k, 1, PatternDataType.Raw));
            using var datagrams = builder.Build();

            Assert.Equal(2, datagrams.NumFrames);
            _ = datagrams[0];
            Assert.Throws<ArgumentOutOfRangeException>(() => datagrams[datagrams.NumFrames]);
        }
    }
}
