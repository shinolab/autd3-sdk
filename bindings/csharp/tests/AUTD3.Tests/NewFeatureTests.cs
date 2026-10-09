using System;
using System.Linq;
using System.Collections.Generic;
using System.Numerics;
using AUTD3;
using Xunit;
using static AUTD3.Holo.HoloUnits;
using static AUTD3.Units;

namespace AUTD3.Tests
{
    using AUTD3.Holo;
    using Holo = AUTD3.Holo.Holo;

    public class NewFeatureTests
    {
        [Fact]
        public void PlaneFillsBuffer()
        {
            using var geometry = Fixture.SingleDevice();
            using var buffer = geometry.PhaseBuffer();
            Pattern.Plane(geometry, new Vector3(0f, 0f, 1f), Pattern.Wavelength(340 * m / s), buffer);
            Assert.Equal(1, buffer.NumDevices);
        }

        [Fact]
        public void BesselFillsBuffer()
        {
            using var geometry = Fixture.SingleDevice();
            using var buffer = geometry.PhaseBuffer();
            Pattern.Bessel(geometry, geometry.Center, new Vector3(0f, 0f, 1f), 0.3f * rad, Pattern.Wavelength(340 * m / s), buffer);
            Assert.Equal(1, buffer.NumDevices);
        }

        [Fact]
        public void LaguerreGaussianMatchesFocusAndWritesIntensity()
        {
            using var geometry = Fixture.SingleDevice();
            var wavelength = Pattern.Wavelength(340 * m / s);
            var device = geometry[0];
            var target = device.Center + new Vector3(0f, 0f, 150f);
            var option = new LaguerreGaussianOption(0, 1, 10f * mm);

            var focused = new Phase[Autd3.NumTransducers];
            Pattern.FocusDevice(device, target, wavelength, focused);

            var fundamental = new Phase[Autd3.NumTransducers];
            Pattern.LaguerreGaussianPhaseDevice(device, target, Vector3.UnitZ, new LaguerreGaussianOption(0, 0, 10f * mm), wavelength, fundamental);
            var offset = (fundamental[0].Value - focused[0].Value + 256) % 256;
            for (var i = 0; i < Autd3.NumTransducers; i++)
            {
                var d = (fundamental[i].Value - focused[i].Value - offset + 512) % 256;
                Assert.True(Math.Min(d, 256 - d) <= 2);
            }

            var lgPhases = new Phase[Autd3.NumTransducers];
            Pattern.LaguerreGaussianPhaseDevice(device, target, Vector3.UnitZ, option, wavelength, lgPhases);
            Assert.Equal(lgPhases[5].Value, Pattern.LaguerreGaussianPhaseTransducer(device.Position(5), target, Vector3.UnitZ, option, wavelength).Value);

            var lgIntensities = new Intensity[Autd3.NumTransducers];
            Pattern.LaguerreGaussianIntensityDevice(device, target, Vector3.UnitZ, option, wavelength, lgIntensities);
            Assert.Equal(255, lgIntensities.Max(e => e.Value));

            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            Pattern.LaguerreGaussianPhase(geometry, target, Vector3.UnitZ, option, wavelength, phases);
            Pattern.LaguerreGaussianIntensity(geometry, target, Vector3.UnitZ, option, wavelength, intensities);
            for (var i = 0; i < Autd3.NumTransducers; i++)
            {
                Assert.Equal(lgPhases[i], phases[0][i]);
                Assert.Equal(lgIntensities[i], intensities[0][i]);
            }
        }

        [Fact]
        public void HermiteGaussianFillsBuffersAndRejectsInvalidWaist()
        {
            using var geometry = Fixture.SingleDevice();
            var wavelength = Pattern.Wavelength(340 * m / s);
            var device = geometry[0];
            var target = device.Center + new Vector3(0f, 0f, 150f);
            var option = new HermiteGaussianOption(1, 0, 10f * mm);

            var hg = new Phase[Autd3.NumTransducers];
            var hgIntensities = new Intensity[Autd3.NumTransducers];
            Pattern.HermiteGaussianPhaseDevice(device, target, Vector3.UnitZ, Vector3.UnitX, option, wavelength, hg);
            Pattern.HermiteGaussianIntensityDevice(device, target, Vector3.UnitZ, Vector3.UnitX, option, wavelength, hgIntensities);
            Assert.Equal(255, hgIntensities.Max(e => e.Value));
            var focusedHg = new Phase[Autd3.NumTransducers];
            Pattern.FocusDevice(device, target, wavelength, focusedHg);
            var baseOffset = (hg[0].Value - focusedHg[0].Value + 256) % 256;
            var flipped = 0;
            for (var i = 0; i < Autd3.NumTransducers; i++)
            {
                var d = (hg[i].Value - focusedHg[i].Value - baseOffset + 512) % 256;
                var toSame = Math.Min(d, 256 - d);
                var toFlip = Math.Abs(d - 128);
                Assert.True(toSame <= 2 || toFlip <= 2);
                if (toFlip <= 2) flipped++;
            }
            Assert.True(flipped > 0);
            Assert.Equal(hg[7].Value, Pattern.HermiteGaussianPhaseTransducer(device.Position(7), target, Vector3.UnitZ, Vector3.UnitX, option, wavelength).Value);

            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            Pattern.HermiteGaussianPhase(geometry, target, Vector3.UnitZ, Vector3.UnitX, option, wavelength, phases);
            Pattern.HermiteGaussianIntensity(geometry, target, Vector3.UnitZ, Vector3.UnitX, option, wavelength, intensities);
            for (var i = 0; i < Autd3.NumTransducers; i++)
            {
                Assert.Equal(hg[i], phases[0][i]);
                Assert.Equal(hgIntensities[i], intensities[0][i]);
            }

            Assert.Throws<Autd3Exception>(() => Pattern.HermiteGaussianPhase(geometry, target, Vector3.UnitZ, Vector3.UnitX, new HermiteGaussianOption(), wavelength, phases));
            Assert.Throws<Autd3Exception>(() => Pattern.LaguerreGaussianIntensity(geometry, target, Vector3.UnitZ, new LaguerreGaussianOption(0, 1, -1f * mm), wavelength, intensities));
        }

        [Fact]
        public void BufferStartsAtZeroPhaseMaxIntensity()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            Assert.All(phases[0], p => Assert.Equal(Phase.Zero, p));
            Assert.All(intensities[0], i => Assert.Equal(Intensity.Max, i));
        }

        [Fact]
        public void SetAndAddPhaseUpdateTheBuffer()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            Pattern.SetIntensity(new Intensity(0x80), intensities);
            Pattern.SetPhase(new Phase(0xF0), phases);
            Pattern.AddPhase(new Phase(0x20), phases);
            Assert.All(phases[0], p => Assert.Equal(new Phase(0x10), p));
            Assert.All(intensities[0], i => Assert.Equal(new Intensity(0x80), i));
        }

        [Fact]
        public void BuffersFromArray()
        {
            var phases = new[] { Enumerable.Range(0, 249).Select(i => new Phase((byte)i)).ToArray() };
            var intensities = new[] { Enumerable.Range(0, 249).Select(i => new Intensity((byte)(255 - i))).ToArray() };
            using var phaseBuffer = PhaseBuffer.FromArray(phases);
            using var intensityBuffer = IntensityBuffer.FromArray(intensities);
            Assert.Equal(1, phaseBuffer.NumDevices);
            Assert.Equal(1, intensityBuffer.NumDevices);
            Assert.Equal(new Phase(5), phaseBuffer[0][5]);
            Assert.Equal(new Intensity(250), intensityBuffer[0][5]);
            Assert.Throws<Autd3Exception>(() => PhaseBuffer.FromArray(new[] { new Phase[10] }));
        }

        [Fact]
        public void BufferIndexerAndIterator()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            Assert.Equal(1, phases.NumDevices);

            var slot = phases[0];
            Assert.Equal(Autd3.NumTransducers, slot.NumTransducers);
            slot[0] = Phase.Pi;
            Assert.Equal(Phase.Pi, slot[0]);

            var intensitySlot = intensities[0];
            intensitySlot[0] = Intensity.Min;
            Assert.Equal(Intensity.Min, intensitySlot[0]);

            var total = 0;
            foreach (var device in phases)
            {
                foreach (var phase in device)
                {
                    total++;
                    _ = phase;
                }
            }
            Assert.Equal(Autd3.NumTransducers, total);

            Assert.Throws<System.ArgumentOutOfRangeException>(() => phases[1]);
            Assert.Throws<System.ArgumentOutOfRangeException>(() => slot[Autd3.NumTransducers]);
            Assert.Throws<System.ArgumentOutOfRangeException>(() => { slot[Autd3.NumTransducers] = Phase.Zero; });
            Assert.Throws<System.ArgumentOutOfRangeException>(() => { intensitySlot[Autd3.NumTransducers] = Intensity.Min; });
        }

        [Fact]
        public void ModulationBufferIndexerAndIterator()
        {
            using var buffer = new ModulationBuffer(10);
            Assert.Equal(10, buffer.Length);
            Assert.All(buffer, b => Assert.Equal(0, b));

            buffer[0] = 0xFF;
            Assert.Equal(0xFF, buffer[0]);

            var total = 0;
            foreach (var sample in buffer)
            {
                total += sample;
            }
            Assert.Equal(0xFF, total);

            Assert.Throws<System.ArgumentOutOfRangeException>(() => buffer[10]);
            Assert.Throws<System.ArgumentOutOfRangeException>(() => { buffer[10] = 0x01; });
        }

        [Fact]
        public void HoloNaiveFillsBuffer()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            var foci = new[]
            {
                new AmplitudeTarget(geometry.Center + new Vector3(0f, 0f, 150f), 150 * dB),
            };
            Holo.Naive(geometry, foci, Pattern.Wavelength(340 * m / s), new NaiveOption { Constraint = IntensityConstraint.Clamp(Intensity.Min, Intensity.Max) }, phases, intensities);
            Assert.Contains(intensities[0], i => i.Value != Intensity.Min.Value);
        }

        [Fact]
        public void SquareProducesSamples()
        {
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Square(200 * Hz, new SquareOption(), modulation);
            Assert.True(modulation.Length > 0);
        }

        [Fact]
        public void FourierProducesSamples()
        {
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Fourier(new[]
            {
                new SineComponent(100 * Hz, new SineOption()),
                new SineComponent(200 * Hz, new SineOption()),
            }, new FourierOption(), modulation);
            Assert.True(modulation.Length > 0);
        }

        [Fact]
        public void RadiationPressureKeepsLength()
        {
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption(), modulation);
            var before = modulation.Length;

            using var pressure = Modulation.ModulationBuffer();
            Modulation.RadiationPressure(modulation, pressure);
            Assert.Equal(before, pressure.Length);
            Assert.Equal(before, modulation.Length);

            Modulation.RadiationPressureInplace(modulation);
            Assert.Equal(before, modulation.Length);
        }

        [Fact]
        public void CircleProducesControlPoints()
        {
            var points = new List<ControlPoints>();
            Stm.Circle(new Vector3(0f, 0f, 150f), 30f * mm, 4, new Vector3(0f, 0f, 1f), Intensity.Max, points);
            Assert.Equal(4, points.Count);
        }

        [Fact]
        public void FociStmBuildsDatagrams()
        {
            using var geometry = Fixture.SingleDevice();
            var points = new List<ControlPoints>();
            Stm.Circle(geometry.Center + new Vector3(0f, 0f, 150f), 30f * mm, 4, new Vector3(0f, 0f, 1f), Intensity.Max, points);
            using var frames = Frames.Encode(geometry, new FociStm(1 * Hz, points.ToArray()));
            Assert.True(frames.Length > 0);
        }

        [Fact]
        public void LaterStagesAModulationBankWithoutChangingIt()
        {
            using var geometry = Fixture.SingleDevice();
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption(), modulation);

            using var frames = Frames.Encode(geometry, new Modulation(ModulationBank.B1, SamplingConfig.Freq4k, modulation,
                transitionMode: TransitionMode.Later));
            Assert.Equal(2, frames.Length);
        }

        [Fact]
        public void LaterStagesAPatternBankWithoutChangingIt()
        {
            using var geometry = Fixture.SingleDevice();
            using var phases = geometry.PhaseBuffer();
            Pattern.SetPhase(Phase.Pi, phases);
            using var intensities = geometry.IntensityBuffer();

            using var frames = Frames.Encode(geometry, new Pattern(PatternBank.B1, phases, intensities, TransitionMode.Later));
            Assert.Equal(2, frames.Length);
        }

        [Fact]
        public void ABankActivationRefusesToNotTransition()
        {
            using var geometry = Fixture.SingleDevice();
            var e = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new ActivateModulationBank(ModulationBank.B1, TransitionMode.Later)));
            Assert.Contains("Later", e.Message);
        }

        [Fact]
        public void CommandsBuildDatagrams()
        {
            using var geometry = Fixture.SingleDevice();
            using var frames = Frames.Encode(geometry, Command.Sequence(
                new Clear(),
                new Synchronize(),
                new ReleaseFailsafe(),
                new ForceFan(true),
                new SetSilencer(new FixedUpdateRate(256, 256)),
                new SetSilencer(),
                SetSilencer.Disable()));
            Assert.Equal(7, frames.Length);
        }

        [Fact]
        public void SetCpuConfigBuildsADatagram()
        {
            using var geometry = Fixture.SingleDevice();

            using var defaultFrames = Frames.Encode(geometry, new SetCpuConfig(new CpuConfig()));
            Assert.Equal(1, defaultFrames.Length);

            using var frames = Frames.Encode(geometry, new SetCpuConfig(new CpuConfig
            {
                SysTimeTransitionMargin = TimeSpan.Zero,
                FpgaWaitUpdateMaxPolls = 1,
                FpgaFlashMaxPolls = uint.MaxValue,
                SyncGuard = TimeSpan.FromTicks(5000),
                UpdateActivateDelay = TimeSpan.FromMilliseconds(200),
                FailsafeTimeout = TimeSpan.FromSeconds(2),
                Ptp = new PtpConfig
                {
                    SyncInterval = TimeSpan.FromMilliseconds(32),
                    TxTimestampTimeout = TimeSpan.FromMilliseconds(4),
                    DelayRespTimeout = TimeSpan.FromMilliseconds(8),
                    Holdover = TimeSpan.FromSeconds(2),
                    LockSamples = 1,
                    StepThreshold = TimeSpan.FromTicks(200),
                    LockThreshold = TimeSpan.FromTicks(2),
                    KpMilli = 200,
                    KiMilli = 40,
                    MaxFreqPpb = 100_000,
                    DelayReqSyncs = 4,
                    PathDelayFilterShift = 0,
                    PauseQuanta = 24,
                    PauseHoldSyncs = 0,
                    PauseRetry = TimeSpan.Zero,
                },
            }));
            Assert.Equal(1, frames.Length);

            using var withoutPauseFrames = Frames.Encode(geometry, new SetCpuConfig(new CpuConfig { Ptp = new PtpConfig { PauseQuanta = null } }));
            Assert.Equal(1, withoutPauseFrames.Length);

            using var disabledFrames = Frames.Encode(geometry, new SetCpuConfig(new CpuConfig { FailsafeTimeout = null }));
            Assert.Equal(1, disabledFrames.Length);
        }

        [Fact]
        public void SetCpuConfigRejectsZeroWhereItIsNotAllowed()
        {
            using var geometry = Fixture.SingleDevice();

            var waitUpdate = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new SetCpuConfig(new CpuConfig { FpgaWaitUpdateMaxPolls = 0 })));
            Assert.Contains("fpgaWaitUpdateMaxPolls", waitUpdate.Message);

            var delayReqSyncs = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new SetCpuConfig(new CpuConfig { Ptp = new PtpConfig { DelayReqSyncs = 0 } })));
            Assert.Contains("ptp.delayReqSyncs", delayReqSyncs.Message);

            var pauseQuanta = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new SetCpuConfig(new CpuConfig { Ptp = new PtpConfig { PauseQuanta = 0 } })));
            Assert.Contains("ptp.pauseQuanta", pauseQuanta.Message);

            var flash = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new SetCpuConfig(new CpuConfig { FpgaFlashMaxPolls = 0 })));
            Assert.Contains("fpgaFlashMaxPolls", flash.Message);

            var lockSamples = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new SetCpuConfig(new CpuConfig { Ptp = new PtpConfig { LockSamples = 0 } })));
            Assert.Contains("ptp.lockSamples", lockSamples.Message);

            var negative = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new SetCpuConfig(new CpuConfig { SyncGuard = TimeSpan.FromTicks(-1) })));
            Assert.Contains("syncGuard", negative.Message);

            var failsafe = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new SetCpuConfig(new CpuConfig { FailsafeTimeout = TimeSpan.Zero })));
            Assert.Contains("failsafeTimeout", failsafe.Message);
        }

        [Fact]
        public void SetCpuConfigRejectsADurationTheWireCannotCarry()
        {
            using var geometry = Fixture.SingleDevice();
            Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry,
                new SetCpuConfig(new CpuConfig { UpdateActivateDelay = TimeSpan.FromTicks(15000) })));

            Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry,
                new SetCpuConfig(new CpuConfig { FailsafeTimeout = TimeSpan.FromTicks(1L << 62) })));
        }

        [Fact]
        public void EachAssignsPerDevice()
        {
            using var geometry = new Geometry(new[]
            {
                new Autd3(Vector3.Zero),
                new Autd3(new Vector3(Autd3.DeviceWidth, 0f, 0f)),
            });
            using var frames = Frames.Encode(geometry,
                Command.Each(device => device.Idx == 0 ? new Clear() : (ICommand?)null));
            Assert.Equal(1, frames.Length);
        }

        [Fact]
        public void SetPulseWidthTableBuildsDatagram()
        {
            using var geometry = Fixture.SingleDevice();
            var table = SetPulseWidthTable.EmptyTable();
            Assert.Equal(SetPulseWidthTable.TableSize, table.Length);
            using var frames = Frames.Encode(geometry, new SetPulseWidthTable(table));
            using var defaultFrames = Frames.Encode(geometry, new SetPulseWidthTable());
            Assert.True(frames.Length > 0);
        }

        [Fact]
        public void PulseWidthFromDuty()
        {
            Assert.Equal(0, PulseWidth.FromDuty(0f).Value);
            Assert.True(PulseWidth.FromDuty(0.5f).Value > 0);
            Assert.Throws<Autd3Exception>(() => PulseWidth.FromDuty(1f));
        }

        [Fact]
        public void DeviceAccessors()
        {
            using var geometry = Fixture.SingleDevice();
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
