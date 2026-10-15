using System;
using System.Linq;
using System.Numerics;
using AUTD3.Holo;
using Xunit;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

namespace AUTD3.Tests
{
    public class ParityAuditTests
    {
        [Fact]
        public void TheDefaultValueOfAValueTypeIsTheRustDefault()
        {
            Assert.Equal(LoopBehavior.Infinite, default(LoopBehavior));
            Assert.Equal((ushort)0xFFFF, default(LoopBehavior).Rep);
            Assert.Equal((ushort)0, LoopBehavior.Once.Rep);
            Assert.Equal((ushort)9, LoopBehavior.Finite(10).Rep);
            Assert.Equal(TransitionMode.Immediate, default(TransitionMode));
            Assert.Equal((byte)0xFF, default(TransitionMode).Mode);
            Assert.Equal((byte)0x00, TransitionMode.SyncIdx.Mode);
            Assert.Equal(Intensity.Max, default(Intensity));
            Assert.Equal(0xFF, default(Intensity).Value);
            Assert.Equal(0x12, new Intensity(0x12).Value);
            Assert.Equal(PhaseDepth.Bits8, default(PhaseDepth));
            Assert.Equal(FpgaBusWait.Cycles3, default(FpgaBusWait));
            Assert.Equal(FpgaBusWait.Cycles3, new CpuConfig().FpgaBusWait);
        }

        [Fact]
        public void AZeroLoopCountIsAnInvalidArgument()
        {
            var error = Assert.Throws<Autd3Exception>(() => LoopBehavior.Finite(0));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
        }

        [Fact]
        public void BothFpgaBusWaitsReachTheNativeConfig()
        {
            foreach (var wait in new[] { FpgaBusWait.Cycles2, FpgaBusWait.Cycles3 })
            {
                var handle = new CpuConfig { FpgaBusWait = wait }.CreateHandle();
                try
                {
                    Assert.Equal(0, NativeCommand.autd3_cpu_config_get_fpga_bus_wait(handle, out var cycles));
                    Assert.Equal(wait == FpgaBusWait.Cycles2 ? (byte)2 : (byte)3, cycles);
                }
                finally
                {
                    NativeCommand.autd3_cpu_config_free(handle);
                }
            }
        }

        [Fact]
        public void BothPhaseDepthsReachTheNativeOp()
        {
            Assert.True(PhaseDepth.Bits4.MaxCount() > PhaseDepth.Bits8.MaxCount());
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            foreach (var depth in new[] { PhaseDepth.Bits8, PhaseDepth.Bits4 })
            {
                using var frames = Frames.Encode(geometry, new PatternStm(1 * Hz, new[] { phases, phases }, Intensity.Max, new PatternStmOption { PhaseDepth = depth }));
                Assert.False(frames.IsEmpty);
            }
        }

        [Fact]
        public void AnUnsetIntensityConstraintIsAnInvalidArgument()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            var foci = new[] { new AmplitudeTarget(geometry.Center + new Vector3(0, 0, 150), 5 * kPa) };
            var option = new NaiveOption { Constraint = default };
            var error = Assert.Throws<Autd3Exception>(() =>
                Holo.Holo.Naive(geometry, foci, Pattern.Wavelength(340 * m / s), option, phases, intensities));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.NotEqual(IntensityConstraint.Normalize, default(IntensityConstraint));
        }

        [Fact]
        public void AHoloErrorKeepsItsCode()
        {
            using var geometry = Fixture.SingleDevice();
            using var other = Fixture.Devices(2);
            using var phases = other.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            var foci = new[] { new AmplitudeTarget(geometry.Center + new Vector3(0, 0, 150), 5 * kPa) };
            var error = Assert.Throws<Autd3Exception>(() =>
                Holo.Holo.Gspat(geometry, foci, Pattern.Wavelength(340 * m / s), new GspatOption(), phases, intensities));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("2 device slots", error.Message);
        }

        [Fact]
        public void ADefaultAutd3IsAtTheOriginWithTheIdentityRotation()
        {
            var device = new Autd3();
            Assert.Equal(Vector3.Zero, device.Origin);
            Assert.Equal(Quaternion.Identity, device.Rotation);
            Assert.Equal(new Autd3(Vector3.Zero), device);
            Assert.NotEqual(new Autd3(new Vector3(1, 0, 0)), device);
            using var geometry = new Geometry(new[] { new Autd3() });
            Assert.Equal(1, geometry.NumDevices);
            Assert.Throws<Autd3Exception>(() => new Geometry(new[] { new Autd3(Vector3.Zero, default) }));
        }

        [Fact]
        public void AnOutOfRangeDeviceIndexThrows()
        {
            using var geometry = Fixture.SingleDevice();
            Assert.Equal("dev", Assert.Throws<ArgumentOutOfRangeException>(() => geometry[1]).ParamName);
            Assert.Equal("dev", Assert.Throws<ArgumentOutOfRangeException>(() => geometry[-1]).ParamName);
            Assert.Equal(0, geometry[0].Idx);
        }

        [Fact]
        public void APerIndexIntensityOfOneBufferIsNotShared()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            var patterns = new[] { phases, phases };

            using var shared = Frames.Encode(geometry, new PatternStm(1 * Hz, patterns, intensities, new PatternStmOption()));
            Assert.False(shared.IsEmpty);

            var perIndex = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new PatternStm(1 * Hz, patterns, new[] { intensities }, new PatternStmOption())));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, perIndex.Code);

            var mismatched = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new PatternStm(1 * Hz, patterns, new[] { intensities, intensities, intensities }, new PatternStmOption())));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, mismatched.Code);

            using var matched = Frames.Encode(geometry, new PatternStm(1 * Hz, patterns, new[] { intensities, intensities }, new PatternStmOption()));
            Assert.False(matched.IsEmpty);
        }

        [Fact]
        public void RadiationPressureRejectsTheSameBuffer()
        {
            using var buffer = ModulationBuffer.FromBytes(new byte[] { 0x80, 0xFF });
            var error = Assert.Throws<Autd3Exception>(() => Modulation.RadiationPressure(buffer, buffer));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Equal(new byte[] { 0x80, 0xFF }, buffer.ToArray());
            using var dst = Modulation.ModulationBuffer();
            Modulation.RadiationPressure(buffer, dst);
            Assert.Equal(2, dst.Length);
        }

        [Fact]
        public void AnUnresolvableModulationSamplingConfigCarriesTheRustMessage()
        {
            using var geometry = Fixture.SingleDevice();
            using var data = ModulationBuffer.FromBytes(new byte[] { 0xFF, 0xFF });
            var error = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new Modulation(new SamplingConfig(39999f * Hz), data)));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("must divide the ultrasound frequency", error.Message);
        }

        [Fact]
        public void APulseWidthIsValidatedWhenItIsUsed()
        {
            var outOfRange = new PulseWidth(512);
            var error = Assert.Throws<Autd3Exception>(() => outOfRange.Value);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("512", error.Message);

            using var geometry = Fixture.SingleDevice();
            var table = SetPulseWidthTable.EmptyTable();
            table[1] = outOfRange;
            var encode = Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry, new SetPulseWidthTable(table)));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, encode.Code);

            Assert.Equal((ushort)256, PulseWidth.FromDuty(0.5f).Value);
            Assert.NotEqual(PulseWidth.FromDuty(0.5f), new PulseWidth(256));
            Assert.Equal(new PulseWidth(256), new PulseWidth(256));
            Assert.Equal(new PulseWidth(0), default(PulseWidth));
        }

        [Fact]
        public void ANegativeCompletionTimeIsRejected()
        {
            using var geometry = Fixture.SingleDevice();
            var error = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new SetSilencer(new FixedCompletionTime { Intensity = TimeSpan.FromTicks(-1) })));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("intensity", error.Message);
        }

        [Fact]
        public void ArgumentErrorsAreInvalidArguments()
        {
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => new SetGpioOut(new GpioOut[3])).Code);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => new EmulateGpioIn(new bool[3])).Code);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => new SetPulseWidthTable(new PulseWidth[1])).Code);

            using var geometry = Fixture.SingleDevice();
            var tooMany = Enumerable.Range(0, Params.NumFociMax + 1).Select(_ => (ControlPoint)Vector3.Zero).ToArray();
            var foci = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new FociStm(1 * Hz, new[] { new ControlPoints(tooMany, Intensity.Max) }, new FociStmOption())));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, foci.Code);
            Assert.Contains($"1..={Params.NumFociMax}", foci.Message);
        }

        [Fact]
        public void SetCpuConfigHasADefaultAndExposesItsConfig()
        {
            var config = new CpuConfig { SyncGuard = TimeSpan.FromMilliseconds(3) };
            Assert.Equal(TimeSpan.FromMilliseconds(3), new SetCpuConfig(config).Config.SyncGuard);
            Assert.Equal(new CpuConfig().SyncGuard, new SetCpuConfig().Config.SyncGuard);
            using var geometry = Fixture.SingleDevice();
            using var frames = Frames.Encode(geometry, new SetCpuConfig());
            Assert.Equal(1, frames.Length);
            Assert.False(frames.IsEmpty);
            using var empty = new Frames();
            Assert.True(empty.IsEmpty);
        }

        [Fact]
        public void FanAndGpioCommandsHaveADefault()
        {
            using var geometry = Fixture.SingleDevice();
            using var fan = Frames.Encode(geometry, new ForceFan());
            Assert.Equal(1, fan.Length);
            using var gpioOut = Frames.Encode(geometry, new SetGpioOut());
            Assert.Equal(1, gpioOut.Length);
            using var gpioIn = Frames.Encode(geometry, new EmulateGpioIn());
            Assert.Equal(1, gpioIn.Length);
        }

        [Fact]
        public void ConstantsAreReadFromTheNativeLibrary()
        {
            Assert.Equal(128, Params.MaxDevices);
            Assert.Equal((ushort)44336, Params.Port);
            Assert.Equal(Params.MaxInflight, Params.DeviceQueueFrames);
            Assert.Equal(1e-3f, Autd3.RotationNormTolerance);
            Assert.Equal(new ServerLink().Port, ServerLink.ServerPort);
            Assert.Equal((ushort)44336, UdpOption.SimulatorPort);
        }

        [Fact]
        public void TheFociStmSoundSpeedDefaultIsReadFromTheNativeLibrary()
        {
            Assert.Equal(FociStmDefaults.SoundSpeed, new FociStmOption().SoundSpeed);
            Assert.Equal(Velocity.FromMS(NativeStm.autd3_foci_stm_option_default_sound_speed()), default(FociStmOption).SoundSpeed);
        }

        [Fact]
        public void ValueTypesCompareLikeRust()
        {
            Assert.True(Phase.Zero == new Phase(0));
            Assert.True(Phase.Pi != Phase.Zero);
            Assert.True(Intensity.Min < Intensity.Max);
            Assert.True(new Intensity(3) >= new Intensity(3));
            Assert.Equal(0, new Intensity(3).CompareTo(new Intensity(3)));

            Assert.True(1 * mm < 1 * m);
            Assert.True(1000 * mm == 1 * m);
            Assert.Equal(30f, (10 * mm + 20 * mm).Mm);
            Assert.Equal(-10f, (10 * mm - 20 * mm).Mm);
            Assert.Equal(-10f, (-(10 * mm)).Mm);
            Assert.Equal(20f, (2 * (10 * mm)).Mm);
            Assert.Equal(20f, ((10 * mm) * 2).Mm);
            Assert.Equal(5f, ((10 * mm) / 2).Mm);

            Assert.Equal(3f, (1 * rad + 2 * rad).Rad);
            Assert.Equal(-1f, (1 * rad - 2 * rad).Rad);
            Assert.Equal(-1f, (-(1 * rad)).Rad);
            Assert.Equal(2f, (2 * (1 * rad)).Rad);
            Assert.Equal(0.5f, ((1 * rad) / 2).Rad);
            Assert.True(1 * rad == Angle.FromRad(1f));
            Assert.Equal(57.29577951308232f, Angle.FromRad(1f).Deg);

            Assert.True(340 * m / s == Velocity.FromMS(340f));
            Assert.True(340 * m / s > 300 * m / s);

            Assert.True(200 * Hz == 200 * Hz);
            Assert.False(200 * Hz == 200f * Hz);
            Assert.True(200f * Hz == 200f * Hz);
            Assert.True(100 * Hz < 200 * Hz);
            Assert.True(100f * Hz < 200 * Hz);
            Assert.Equal(200 * Hz, 200 * Hz);

            Assert.True(2 * kPa == 2000 * Pa);
            Assert.True(1 * kPa < 2 * kPa);
            Assert.Equal(4000f, (2 * kPa * 2).Pascal);
            Assert.Equal(4000f, (2 * (2 * kPa)).Pascal);
            Assert.Equal(1000f, (2 * kPa / 2).Pascal);
            Assert.Equal(new AmplitudeTarget(Vector3.Zero, 1 * kPa), new AmplitudeTarget(Vector3.Zero, 1000 * Pa));

            Assert.True(Interface.Auto == default(Interface));
            Assert.True(Interface.Name("eth0") == Interface.Name("eth0"));
            Assert.True(Interface.Name("eth0") != Interface.Addr("eth0"));
            Assert.True(LoopBehavior.Finite(2) == LoopBehavior.Finite(2));
            Assert.True(LoopBehavior.Once != LoopBehavior.Infinite);
            Assert.True(TransitionMode.SysTime(SysTime.FromNanos(1)) != TransitionMode.SysTime(SysTime.FromNanos(2)));
            Assert.True(TransitionMode.Gpio(GpioIn.I1) == TransitionMode.Gpio(GpioIn.I1));
        }

        [Fact]
        public void ASineComponentTellsNearestFromExact()
        {
            Assert.True(new SineComponent(Nearest(150.5f * Hz), new SineOption()).IsNearest);
            Assert.False(new SineComponent(150.5f * Hz, new SineOption()).IsNearest);
        }

        [Fact]
        public void ControlPointsConvertFromPointsLikeRustFrom()
        {
            var point = new Vector3(1, 2, 3);
            var cp = (ControlPoint)point;
            Assert.Equal(point, cp.Point);
            Assert.Equal(Phase.Zero, cp.PhaseOffset);
            var cps = (ControlPoints)point;
            Assert.Equal(Intensity.Max, cps.Intensity);
            Assert.Single(cps.Points);
            Assert.Equal(2, ((ControlPoints)new[] { point, point }).Points.Length);
        }

        [Fact]
        public void GroupDeviceWritesTheSourceOfEachKeyForOneDevice()
        {
            using var geometry = Fixture.Devices(2);
            var groups = new TransducerGroups<int>(geometry, (device, tr) => device.Idx == 1 && tr % 3 == 1 ? 1 : 0);
            using var zero = geometry.PhaseBuffer();
            using var one = geometry.PhaseBuffer();
            Pattern.SetPhase(new Phase(0x10), zero);
            Pattern.SetPhase(new Phase(0x20), one);
            var dst = new Phase[Autd3.NumTransducers];
            Pattern.GroupDevice(geometry[1], groups, key => key == 0 ? zero : one, dst);
            for (var tr = 0; tr < dst.Length; tr++)
            {
                Assert.Equal(tr % 3 == 1 ? new Phase(0x20) : new Phase(0x10), dst[tr]);
            }

            using var low = geometry.IntensityBuffer();
            using var high = geometry.IntensityBuffer();
            Pattern.SetIntensity(new Intensity(0x30), low);
            Pattern.SetIntensity(new Intensity(0x40), high);
            var dstI = new Intensity[Autd3.NumTransducers];
            Pattern.GroupDevice(geometry[0], groups, key => key == 0 ? low : high, dstI);
            Assert.All(dstI, i => Assert.Equal(new Intensity(0x30), i));

            using var single = Fixture.SingleDevice();
            using var mismatched = single.PhaseBuffer();
            Assert.Throws<Autd3Exception>(() => Pattern.GroupDevice(geometry[0], groups, key => mismatched, dst));
        }

        [Fact]
        public void AnEmulatorReportsItsDeviceCount()
        {
            using var emulator = new UdpEmulator(2);
            Assert.Equal(2, emulator.NumDevices);
        }
    }
}
