using System;
using System.Linq;
using System.Numerics;
using AUTD3;
using Xunit;
using static AUTD3.Units;

namespace AUTD3.Tests
{
    public class DriftParityTests
    {
        [Fact]
        public void SamplingConfigConstructors()
        {
            Assert.True(new SamplingConfig(512).Divide() == 512);
            Assert.InRange(new SamplingConfig(4000 * Hz).Freq().Hz, 3999f, 4001f);
            Assert.Equal(TimeSpan.FromMilliseconds(1), new SamplingConfig(TimeSpan.FromMilliseconds(1)).Period());
            Assert.True(new SamplingConfig(Nearest(4001.5f * Hz)).Divide() > 0);
            Assert.True(new SamplingConfig(Nearest(TimeSpan.FromTicks(2501))).Divide() > 0);
            Assert.Throws<Autd3Exception>(() => new SamplingConfig(0));
        }

        [Fact]
        public void AnStmSamplingConfigIsTheLoopFrequencyTimesTheSampleCount()
        {
            Assert.Equal(40000, new SamplingConfig(1 * Hz * 1).Divide());
            Assert.Equal(100, new SamplingConfig(100.0f * Hz * 4).Divide());
            Assert.Equal(10, new SamplingConfig(TimeSpan.FromMilliseconds(1) / 4).Divide());
            Assert.True(new SamplingConfig(Nearest(4001.0f * Hz)).Divide() > 0);
            Assert.Throws<Autd3Exception>(() => new SamplingConfig(4001.0f * Hz));
        }

        [Fact]
        public void PhaseDepthMaxCount()
        {
            Assert.Equal(5, PhaseDepth.Bits8.MaxCount());
            Assert.Equal(11, PhaseDepth.Bits4.MaxCount());
        }

        [Fact]
        public void DeviceIsEmpty()
        {
            using var geometry = Fixture.SingleDevice();
            Assert.False(geometry[0].IsEmpty);
        }

        [Fact]
        public void GeometryIterationAndDeviceCenter()
        {
            using var geometry = new Geometry(new[]
            {
                new Autd3(Vector3.Zero),
                new Autd3(new Vector3(Autd3.DeviceWidth, 0f, 0f)),
            });
            Assert.False(geometry.IsEmpty);
            Assert.Equal(2, geometry.Count());
            var center = geometry[0].Center;
            Assert.True(center.X > 0f && center.Y > 0f);
        }

        [Fact]
        public void PatternDeviceAndTransducerVariants()
        {
            using var geometry = Fixture.SingleDevice();
            var wavelength = Pattern.Wavelength(340 * m / s);
            var device = geometry[0];
            var target = device.Center + new Vector3(0f, 0f, 150f);

            var dst = new Phase[Autd3.NumTransducers];
            Pattern.FocusDevice(device, target, wavelength, dst);
            var e = Pattern.FocusTransducer(device.Position(0), target, wavelength);
            Assert.Equal(e, dst[0]);

            Pattern.PlaneDevice(device, new Vector3(0f, 0f, 1f), wavelength, dst);
            var pe = Pattern.PlaneTransducer(device.Position(0), new Vector3(0f, 0f, 1f), wavelength);
            Assert.Equal(pe, dst[0]);

            Pattern.BesselDevice(device, device.Center, new Vector3(0f, 0f, 1f), 0.3f * rad, wavelength, dst);
            var be = Pattern.BesselTransducer(device.Position(0), device.Center, new Vector3(0f, 0f, 1f), 0.3f * rad, wavelength);
            Assert.Equal(be, dst[0]);

            Assert.Throws<Autd3Exception>(() => Pattern.FocusDevice(device, target, wavelength, new Phase[10]));
        }

        [Fact]
        public void SysTimeRoundTrips()
        {
            var t = SysTime.FromNanos(836_352_000_000_000_000);
            Assert.Equal(0UL, SysTime.Zero.Nanos);
            Assert.Equal(1000UL, SysTime.FromNanos(1000).Nanos);
            Assert.Equal(t + TimeSpan.FromSeconds(1) - TimeSpan.FromSeconds(1), t);
            _ = TransitionMode.SysTime(t);
            _ = GpioOut.SysTimeEq(t);
        }

        [Fact]
        public void PhaseAndIntensityOperators()
        {
            Assert.Equal(Phase.Zero.Value, (Phase.Pi + Phase.Pi).Value);
            Assert.Equal(0x40, (Phase.Pi / 2).Value);
            Assert.Equal(Phase.Pi.Value, ((Phase)(MathF.PI * rad)).Value);
            Assert.Equal(Intensity.Max.Value, (Intensity.Max + Intensity.Max).Value);
            Assert.Equal(Intensity.Min.Value, (Intensity.Min - Intensity.Max).Value);
            Assert.Equal(0x80, (new Intensity(0x40) * 2).Value);
        }

        [Fact]
        public void UnitAccessors()
        {
            Assert.Equal(1f, (1000 * mm).M);
            Assert.Equal(90f, Angle.FromDeg(90f).Deg, 3);
            Assert.Equal((MathF.PI * rad).Rad, Angle.FromRad(MathF.PI).Rad);
            Assert.Equal(0f, Angle.Zero.Rad);
            Assert.Equal(340f, Velocity.FromMS(340f).MS);
            Assert.Equal(340000f, Velocity.FromMmS(340000f).MmS);
            Assert.Equal(5f, Length.FromMm(5f).Mm);
            Assert.Equal(300f, ((200 * Hz) + (100 * Hz)).Hz);
            Assert.Equal(100f, ((200 * Hz) - (100 * Hz)).Hz);
            Assert.Equal(400f, ((200 * Hz) * 2u).Hz);
            Assert.Equal(100f, ((200 * Hz) / 2u).Hz);
        }

        [Fact]
        public void ConfigFociStmBuildsDatagram()
        {
            using var geometry = Fixture.SingleDevice();
            using var frames = Frames.Encode(geometry, new ConfigFociStm(PatternBank.B0, SamplingConfig.Freq4k, 4, 1, Velocity.FromMS(340f), LoopBehavior.Infinite));
            Assert.True(frames.Length > 0);
        }

        [Fact]
        public void InterfaceFactories()
        {
            _ = Interface.Auto;
            _ = Interface.Name("eth0");
            _ = Interface.Addr("[::1]:44336");
        }

        [Fact]
        public void DeviceStateToStringMatchesRust()
        {
            Assert.Equal("READY", DeviceState.Ready.ToString());
            Assert.Equal("SYNCING", DeviceState.Syncing.ToString());
            Assert.Equal("LOST", DeviceState.Lost.ToString());
            Assert.True(DeviceState.Ready == DeviceState.Ready);
            Assert.True(DeviceState.Ready != DeviceState.Lost);
        }
    }
}
