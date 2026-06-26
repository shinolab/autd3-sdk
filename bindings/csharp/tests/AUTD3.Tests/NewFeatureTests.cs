using System.Numerics;
using AUTD3;
using Xunit;

namespace AUTD3.Tests
{
    public class NewFeatureTests
    {
        private static Geometry SingleDevice() => new Geometry(new[] { new Device(Vector3.Zero) });

        [Fact]
        public void PlaneFillsBuffer()
        {
            using var geometry = SingleDevice();
            using var buffer = new PatternBuffer(geometry);
            Pattern.Plane(geometry, new Vector3(0f, 0f, 1f), Pattern.Wavelength(340f * 1000f), new PlaneOption(), buffer);
            Assert.Equal(1, buffer.NumDevices);
        }

        [Fact]
        public void BesselFillsBuffer()
        {
            using var geometry = SingleDevice();
            using var buffer = new PatternBuffer(geometry);
            Pattern.Bessel(geometry, geometry.Center, new Vector3(0f, 0f, 1f), 0.3f, Pattern.Wavelength(340f * 1000f), new BesselOption(), buffer);
            Assert.Equal(1, buffer.NumDevices);
        }

        [Fact]
        public void UniformFillsBuffer()
        {
            using var geometry = SingleDevice();
            using var buffer = new PatternBuffer(geometry);
            Pattern.Uniform(Phase.Pi, Intensity.Max, buffer);
            Assert.Equal(1, buffer.NumDevices);
        }

        [Fact]
        public void HoloNaiveFillsBuffer()
        {
            using var geometry = SingleDevice();
            using var buffer = new PatternBuffer(geometry);
            var foci = new[]
            {
                new HoloControlPoint(geometry.Center + new Vector3(0f, 0f, 150f), Amplitude.FromSpl(150f)),
            };
            Holo.Naive(geometry, foci, Pattern.Wavelength(340f * 1000f), EmissionConstraint.Clamp(Intensity.Min, Intensity.Max), Directivity.Sphere, buffer);
            Assert.Equal(1, buffer.NumDevices);
        }

        [Fact]
        public void SquareProducesSamples()
        {
            using var modulation = new ModulationBuffer();
            Modulation.Square(200f, new SquareOption(), modulation);
            Assert.True(modulation.Length > 0);
        }

        [Fact]
        public void FourierProducesSamples()
        {
            using var modulation = new ModulationBuffer();
            Modulation.Fourier(new[]
            {
                new SineComponent(100f, new SineOption()),
                new SineComponent(200f, new SineOption()),
            }, new FourierOption(), modulation);
            Assert.True(modulation.Length > 0);
        }

        [Fact]
        public void RadiationPressureKeepsLength()
        {
            using var modulation = new ModulationBuffer();
            Modulation.Sine(200f, new SineOption(), modulation);
            var before = modulation.Length;
            Modulation.RadiationPressure(modulation);
            Assert.Equal(before, modulation.Length);
        }

        [Fact]
        public void CircleProducesControlPoints()
        {
            var points = Stm.Circle(new Vector3(0f, 0f, 150f), 30f, 4, new Vector3(0f, 0f, 1f));
            Assert.Equal(4, points.Length);
        }

        [Fact]
        public void FociStmBuildsDatagrams()
        {
            using var geometry = SingleDevice();
            var points = Stm.Circle(geometry.Center + new Vector3(0f, 0f, 150f), 30f, 4, new Vector3(0f, 0f, 1f));
            using var builder = new DatagramBuilder(geometry);
            builder.Push(new FociStm(StmConfig.Freq(1f), points));
            using var datagrams = builder.Build();
            Assert.True(datagrams.NumFrames > 0);
        }

        [Fact]
        public void CommandsBuildDatagrams()
        {
            using var geometry = SingleDevice();
            using var builder = new DatagramBuilder(geometry);
            builder
                .Push(new Clear())
                .Push(new Synchronize())
                .Push(new ForceFan(true))
                .Push(SetSilencer.FromUpdateRate(256, 256))
                .Push(SetSilencer.Disable());
            using var datagrams = builder.Build();
            Assert.True(datagrams.NumFrames > 0);
        }

        [Fact]
        public void SetPulseWidthTableBuildsDatagram()
        {
            using var geometry = SingleDevice();
            var table = PulseWidth.DefaultTable();
            Assert.Equal(PulseWidth.TableSize, table.Length);
            using var builder = new DatagramBuilder(geometry);
            builder.Push(new SetPulseWidthTable(table));
            using var datagrams = builder.Build();
            Assert.True(datagrams.NumFrames > 0);
        }

        [Fact]
        public void PulseWidthFromDuty()
        {
            Assert.Equal(0, PulseWidth.FromDuty(0f));
            Assert.True(PulseWidth.FromDuty(0.5f) > 0);
            Assert.Throws<Autd3Exception>(() => PulseWidth.FromDuty(1f));
        }

        [Fact]
        public void DeviceAccessors()
        {
            using var geometry = SingleDevice();
            Assert.True(geometry.NumTransducers > 0);
            var device = geometry[0];
            Assert.Equal(0, device.Idx);
            Assert.True(device.NumTransducers > 0);
            Assert.Equal(geometry.NumTransducers, device.NumTransducers);
            var rotation = device.Rotation;
            Assert.Equal(Quaternion.Identity, rotation);
            Assert.Equal(1f, device.XDirection.Length(), 3);
            Assert.Equal(1f, device.YDirection.Length(), 3);
            Assert.Equal(1f, device.AxialDirection.Length(), 3);
        }
    }
}
