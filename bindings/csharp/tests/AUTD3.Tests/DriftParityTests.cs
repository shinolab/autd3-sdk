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
        private static Geometry SingleDevice() => new Geometry(new[] { new Autd3(Vector3.Zero) });

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
        public void StmConfigConstructors()
        {
            _ = new StmConfig(1 * Hz);
            _ = new StmConfig(Nearest(1.5f * Hz));
            _ = new StmConfig(TimeSpan.FromSeconds(1));
            _ = new StmConfig(Nearest(TimeSpan.FromSeconds(1)));
            _ = new StmConfig(SamplingConfig.Freq4k);
        }

        [Fact]
        public void StmConfigIntoSamplingConfig()
        {
            Assert.Equal(100, new StmConfig(100.0f * Hz).IntoSamplingConfig(4).Divide());
            Assert.Equal(10, new StmConfig(TimeSpan.FromMilliseconds(1)).IntoSamplingConfig(4).Divide());
            Assert.Equal(10, new StmConfig(SamplingConfig.Freq4k).IntoSamplingConfig(7).Divide());
            Assert.True(new StmConfig(Nearest(4001.0f * Hz)).IntoSamplingConfig(1).Divide() > 0);
            Assert.Throws<Autd3Exception>(() => new StmConfig(4001.0f * Hz).IntoSamplingConfig(1));
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
            using var geometry = SingleDevice();
            var wavelength = Pattern.Wavelength(340 * m / s);
            var device = geometry[0];
            var target = device.Center + new Vector3(0f, 0f, 150f);

            var dst = new Emission[Autd3.NumTransducers];
            Pattern.FocusDevice(device, target, wavelength, new FocusOption(Intensity.Max), dst);
            var e = Pattern.FocusTransducer(device.Position(0), target, wavelength, new FocusOption(Intensity.Max));
            Assert.Equal(e.Phase.Value, dst[0].Phase.Value);
            Assert.Equal(Intensity.Max.Value, dst[0].Intensity.Value);

            Pattern.PlaneDevice(device, new Vector3(0f, 0f, 1f), wavelength, new PlaneOption(Intensity.Max), dst);
            var pe = Pattern.PlaneTransducer(device.Position(0), new Vector3(0f, 0f, 1f), wavelength, new PlaneOption(Intensity.Max));
            Assert.Equal(pe.Phase.Value, dst[0].Phase.Value);

            Pattern.BesselDevice(device, device.Center, new Vector3(0f, 0f, 1f), 0.3f * rad, wavelength, new BesselOption(Intensity.Max), dst);
            var be = Pattern.BesselTransducer(device.Position(0), device.Center, new Vector3(0f, 0f, 1f), 0.3f * rad, wavelength, new BesselOption(Intensity.Max));
            Assert.Equal(be.Phase.Value, dst[0].Phase.Value);

            Pattern.UniformDevice(new Emission(Phase.Pi, Intensity.Max), dst);
            Assert.Equal(Phase.Pi.Value, dst[10].Phase.Value);

            Pattern.NullDevice(dst);
            Assert.Equal(Intensity.Min.Value, dst[10].Intensity.Value);

            Pattern.NullTransducer(ref dst[0]);
            Assert.Equal(Phase.Zero.Value, dst[0].Phase.Value);
        }

        [Fact]
        public void DcSysTimeRoundTrips()
        {
            var utc = new DateTime(2026, 7, 3, 0, 0, 0, DateTimeKind.Utc);
            var t = DcSysTime.FromUtc(utc);
            Assert.Equal(utc, t.ToUtc());
            Assert.Equal(0UL, DcSysTime.Zero.SysTime);
            Assert.Equal(1000UL, DcSysTime.FromNanos(1000).SysTime);
            Assert.True(DcSysTime.Now() > DcSysTime.Zero);
            Assert.Equal(t + TimeSpan.FromSeconds(1) - TimeSpan.FromSeconds(1), t);
            Assert.Throws<Autd3Exception>(() => DcSysTime.FromUtc(new DateTime(1999, 12, 31, 0, 0, 0, DateTimeKind.Utc)));
            _ = TransitionMode.SysTime(t);
        }

        [Fact]
        public void PhaseAndIntensityOperators()
        {
            Assert.Equal(Phase.Zero.Value, (Phase.Pi + Phase.Pi).Value);
            Assert.Equal(0x40, (Phase.Pi / 2).Value);
            Assert.Equal(Phase.Pi.Value, ((Phase)Angle.Pi).Value);
            Assert.Equal(Intensity.Max.Value, (Intensity.Max + Intensity.Max).Value);
            Assert.Equal(Intensity.Min.Value, (Intensity.Min - Intensity.Max).Value);
            Assert.Equal(0x80, (new Intensity(0x40) * 2).Value);
        }

        [Fact]
        public void UnitAccessors()
        {
            Assert.Equal(1f, (1000 * mm).M);
            Assert.Equal(90f, Angle.FromDegree(90f).Degree, 3);
            Assert.Equal(Angle.Pi.Radian, Angle.FromRadian(MathF.PI).Radian);
            Assert.Equal(0f, Angle.Zero.Radian);
            Assert.Equal(340f, Velocity.FromMS(340f).MPerS);
            Assert.Equal(340000f, Velocity.FromMmS(340000f).MmPerS);
            Assert.Equal(5f, Length.Millimeters(5f).Mm);
            Assert.Equal(300f, ((200 * Hz) + (100 * Hz)).Hz);
            Assert.Equal(100f, ((200 * Hz) - (100 * Hz)).Hz);
            Assert.Equal(400f, ((200 * Hz) * 2u).Hz);
            Assert.Equal(100f, ((200 * Hz) / 2u).Hz);
        }

        [Fact]
        public void ConfigFociStmBuildsDatagram()
        {
            using var geometry = SingleDevice();
            using var builder = new DatagramBuilder(geometry);
            builder.Push(new ConfigFociStm(PatternBank.B0, SamplingConfig.Freq4k, 4, 1, Velocity.FromMS(340f)));
            using var frames = builder.Build();
            Assert.True(frames.Length > 0);
        }

        [Fact]
        public void InterfaceFactories()
        {
            _ = Interface.Auto;
            _ = Interface.Name("eth0");
        }

        [Fact]
        public async System.Threading.Tasks.Task OpenWithCheckerReportsStatus()
        {
            using var geometry = SingleDevice();
            var (client, checker) = await Client.OpenWithCheckerAsync(geometry, new AUTD3.Link.Nop(), new ClientConfig());
            using var c = client;
            using var k = checker;
            var status = await checker.CheckAsync();
            Assert.Equal(DeviceState.Op, Assert.Single(status.Devices));
            Assert.True(status.AllOp);
            Assert.False(status.AnyLost);
            await client.CloseAsync();
        }

        [Fact]
        public async System.Threading.Tasks.Task ResponseTokenIsDirectlyAwaitable()
        {
            using var geometry = SingleDevice();
            using var client = await Client.OpenAsync(geometry, new AUTD3.Link.Nop(), new ClientConfig());
            using var builder = client.DatagramBuilder();
            builder.Push(new Synchronize());
            using var frames = builder.Build();
            var token = await client.SendAsync(frames[0]);
            var response = await token;
            Assert.Equal(client.NumDevices, response.Data.Count);
            response.Check();
            await Assert.ThrowsAsync<Autd3Exception>(async () => await token);
            await client.CloseAsync();
        }

        [Fact]
        public void DeviceStateToStringMatchesRust()
        {
            Assert.Equal("OP", DeviceState.Op.ToString());
            Assert.Equal("SAFE-OP", DeviceState.SafeOp.ToString());
            Assert.Equal("SAFE-OP + ERROR", DeviceState.SafeOpError.ToString());
            Assert.Equal("LOST", DeviceState.Lost.ToString());
            Assert.Equal("INIT", DeviceState.Other(0x01).ToString());
            Assert.Equal("UNKNOWN (0x0a)", DeviceState.Other(0x0A).ToString());
            Assert.True(DeviceState.Op == DeviceState.Op);
            Assert.True(DeviceState.Op != DeviceState.Lost);
        }
    }
}
