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
        public void EveryCpuConfigFieldReachesItsOwnNativeField()
        {
            var native = new CpuConfig
            {
                SyncGuard = TimeSpan.FromTicks(1),
                FailsafeTimeout = TimeSpan.FromTicks(2),
                PtpUnlockFailsafeTimeout = TimeSpan.FromTicks(3),
                FpgaBusWait = FpgaBusWait.Cycles2,
                CpuWatchdogTimeout = null,
                Ptp = new PtpConfig
                {
                    SyncInterval = TimeSpan.FromTicks(4),
                    TxTimestampTimeout = TimeSpan.FromTicks(5),
                    DelayRespTimeout = TimeSpan.FromTicks(6),
                    Holdover = TimeSpan.FromTicks(7),
                    LockSamples = 8,
                    StepThreshold = TimeSpan.FromTicks(9),
                    LockThreshold = TimeSpan.FromTicks(10),
                    KpMilli = 11,
                    KiMilli = 12,
                    MaxFreqPpb = 13,
                    DelayReqSyncs = 14,
                    PathDelayFilterShift = 15,
                    PauseQuanta = 16,
                    PauseHoldSyncs = 17,
                    PauseRetry = TimeSpan.FromTicks(18),
                },
            }.ToNative();
            Assert.Equal(100ul, native.SyncGuardNs);
            Assert.Equal((byte)1, native.HasFailsafeTimeout);
            Assert.Equal(200ul, native.FailsafeTimeoutNs);
            Assert.Equal((byte)1, native.HasPtpUnlockFailsafeTimeout);
            Assert.Equal(300ul, native.PtpUnlockFailsafeTimeoutNs);
            Assert.Equal((byte)2, native.FpgaBusWait);
            Assert.Equal((byte)0, native.HasCpuWatchdogTimeout);
            Assert.Equal(400ul, native.Ptp.SyncIntervalNs);
            Assert.Equal(500ul, native.Ptp.TxTimestampTimeoutNs);
            Assert.Equal(600ul, native.Ptp.DelayRespTimeoutNs);
            Assert.Equal(700ul, native.Ptp.HoldoverNs);
            Assert.Equal((ushort)8, native.Ptp.LockSamples);
            Assert.Equal(900ul, native.Ptp.StepThresholdNs);
            Assert.Equal(1000ul, native.Ptp.LockThresholdNs);
            Assert.Equal(11u, native.Ptp.KpMilli);
            Assert.Equal(12u, native.Ptp.KiMilli);
            Assert.Equal(13u, native.Ptp.MaxFreqPpb);
            Assert.Equal((ushort)14, native.Ptp.DelayReqSyncs);
            Assert.Equal((byte)15, native.Ptp.PathDelayFilterShift);
            Assert.Equal((ushort)16, native.Ptp.PauseQuanta);
            Assert.Equal((ushort)17, native.Ptp.PauseHoldSyncs);
            Assert.Equal(1800ul, native.Ptp.PauseRetryNs);

            var err = new byte[NativeAbi.ErrorBufferLength];
            var op = NativeCommand.autd3_op_set_cpu_config(native, err, (UIntPtr)err.Length);
            Assert.NotEqual(IntPtr.Zero, op);
            NativeClient.autd3_op_free(op);
        }

        [Fact]
        public void BothFpgaBusWaitsReachTheNativeConfig()
        {
            foreach (var wait in new[] { FpgaBusWait.Cycles2, FpgaBusWait.Cycles3 })
            {
                var cycles = new CpuConfig { FpgaBusWait = wait }.ToNative().FpgaBusWait;
                Assert.Equal(wait == FpgaBusWait.Cycles2 ? (byte)2 : (byte)3, cycles);
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
                using var frames = Frames.Encode(geometry, new PatternStm(new SamplingConfig(1 * Hz * 2), new[] { phases, phases }, Intensity.Max, new PatternStmOption { PhaseDepth = depth }));
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
            Assert.NotEqual(IntensityConstraint.Multiply(0f), default(IntensityConstraint));
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

            using var shared = Frames.Encode(geometry, new PatternStm(new SamplingConfig(1 * Hz * patterns.Length), patterns, intensities, new PatternStmOption()));
            Assert.False(shared.IsEmpty);

            var perIndex = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new PatternStm(new SamplingConfig(1 * Hz * patterns.Length), patterns, new[] { intensities }, new PatternStmOption())));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, perIndex.Code);

            var mismatched = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new PatternStm(new SamplingConfig(1 * Hz * patterns.Length), patterns, new[] { intensities, intensities, intensities }, new PatternStmOption())));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, mismatched.Code);

            using var matched = Frames.Encode(geometry, new PatternStm(new SamplingConfig(1 * Hz * patterns.Length), patterns, new[] { intensities, intensities }, new PatternStmOption()));
            Assert.False(matched.IsEmpty);
        }

        [Fact]
        public void AnUnresolvableModulationSamplingConfigCarriesTheRustMessage()
        {
            using var geometry = Fixture.SingleDevice();
            using var data = ModulationBuffer.FromBytes(new byte[] { 0xFF, 0xFF });
            var error = Assert.Throws<Autd3Exception>(() => new Modulation(new SamplingConfig(39999f * Hz), data));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("must divide the ultrasound frequency", error.Message);
        }

        [Fact]
        public void APulseWidthIsValidatedWhenItIsConstructed()
        {
            var error = Assert.Throws<Autd3Exception>(() => new PulseWidth(512));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("512", error.Message);

            using var geometry = Fixture.SingleDevice();
            var table = SetPulseWidthTable.EmptyTable();
            table[1] = new PulseWidth(511);
            using var frames = Frames.Encode(geometry, new SetPulseWidthTable(table));
            Assert.False(frames.IsEmpty);

            Assert.Equal((ushort)256, PulseWidth.FromDuty(0.5f).Value);
            Assert.Equal(PulseWidth.FromDuty(0.5f), new PulseWidth(256));
            Assert.Equal(new PulseWidth(256), new PulseWidth(256));
            Assert.NotEqual(new PulseWidth(255), new PulseWidth(256));
            Assert.Equal(new PulseWidth(256).GetHashCode(), PulseWidth.FromDuty(0.5f).GetHashCode());
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
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => ExecAt.Lead(TimeSpan.FromTicks(-1))).Code);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => new EmulateGpioIn(new bool[3])).Code);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => new SetPulseWidthTable(new PulseWidth[1])).Code);

            using var geometry = Fixture.SingleDevice();
            var tooMany = Enumerable.Range(0, Params.NumFociMax + 1).Select(_ => (ControlPoint)Vector3.Zero).ToArray();
            var foci = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new FociStm(new SamplingConfig(1 * Hz), new[] { new ControlPoints(tooMany, Intensity.Max) }, new FociStmOption())));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, foci.Code);
            Assert.Contains($"1..={Params.NumFociMax}", foci.Message);
        }

        [Fact]
        public void ExecAtCarriesItsKindAndNanosecondsToTheNative()
        {
            var immediate = ExecAt.Immediate.ToNative();
            Assert.Equal((0, 0UL), (immediate.Kind, immediate.Ns));
            var lead = ExecAt.Lead(TimeSpan.FromMilliseconds(1.5)).ToNative();
            Assert.Equal((1, 1_500_000UL), (lead.Kind, lead.Ns));
            var at = ExecAt.At(SysTime.FromNanos(123_456_789)).ToNative();
            Assert.Equal((2, 123_456_789UL), (at.Kind, at.Ns));
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
        public void SetFpgaConfigHasADefaultAndExposesItsConfig()
        {
            var config = new FpgaConfig { SyncPhaseLock = false };
            Assert.False(new SetFpgaConfig(config).Config.SyncPhaseLock);
            Assert.Equal(new FpgaConfig().SyncPhaseLock, new SetFpgaConfig().Config.SyncPhaseLock);
            using var geometry = Fixture.SingleDevice();
            using var frames = Frames.Encode(geometry, new SetFpgaConfig());
            Assert.Equal(1, frames.Length);
        }

        [Fact]
        public void BothSyncPhaseLockValuesReachTheNativeConfig()
        {
            foreach (var syncPhaseLock in new[] { false, true })
            {
                var value = new FpgaConfig { SyncPhaseLock = syncPhaseLock }.ToNative().SyncPhaseLock;
                Assert.Equal(syncPhaseLock, value != 0);
            }
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
            Assert.Equal(1e-3f, Autd3.RotationNormTolerance);
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
            Assert.True(200 * Hz == 200f * Hz);
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
    }
}
