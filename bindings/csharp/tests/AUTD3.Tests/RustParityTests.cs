using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3.Holo;
using Xunit;
using static AUTD3.Holo.HoloUnits;
using static AUTD3.Units;

namespace AUTD3.Tests
{
    public class RustParityTests
    {
        [Theory]
        [InlineData(100.0, 4, 1000)]
        [InlineData(333.0, 3, 4440)]
        [InlineData(1.0, 2, 20)]
        public void AnStmPeriodKeepsNanosecondPrecision(double millis, int size, int divide)
        {
            var config = new StmConfig(TimeSpan.FromMilliseconds(millis));
            Assert.Equal((ushort)divide, config.IntoSamplingConfig(size).Divide());

            using var geometry = Fixture.SingleDevice();
            var points = new List<ControlPoints>();
            Stm.Circle(geometry.Center + new Vector3(0f, 0f, 150f), 30f * mm, size, new Vector3(0f, 0f, 1f), Intensity.Max, points);
            using var frames = Frames.Encode(geometry, new FociStm(TimeSpan.FromMilliseconds(millis), points));
            Assert.True(frames.Length > 0);
        }

        [Fact]
        public void AnIndivisibleStmPeriodCarriesTheRustMessage()
        {
            var config = new StmConfig(TimeSpan.FromMilliseconds(100));
            var e = Assert.Throws<Autd3Exception>(() => config.IntoSamplingConfig(3));
            Assert.Contains("must be divisible by the number of samples", e.Message);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, e.Code);
        }

        [Fact]
        public void TheSilencerDefaultsAreReadFromTheNativeLibrary()
        {
            var completion = new FixedCompletionTime();
            Assert.Equal(TimeSpan.FromTicks(Params.UltrasoundPeriod.Ticks * 10), completion.Intensity);
            Assert.Equal(TimeSpan.FromTicks(Params.UltrasoundPeriod.Ticks * 40), completion.Phase);
            Assert.True(completion.StrictMode);
            Assert.Equal(completion.Intensity, default(FixedCompletionTime).Intensity);
            Assert.True(default(FixedCompletionTime).StrictMode);
            Assert.False(new FixedCompletionTime { StrictMode = false }.StrictMode);

        }

        [Fact]
        public void TheUpdateRateHasNoDefault()
        {
            using var geometry = Fixture.SingleDevice();
            var rate = new FixedUpdateRate(256, 8);
            Assert.Equal((ushort)256, rate.Intensity);
            Assert.Equal((ushort)8, rate.Phase);
            using var frames = Frames.Encode(geometry, new SetSilencer(rate));
            Assert.Equal(1, frames.Length);

            Assert.Equal((ushort)0, default(FixedUpdateRate).Intensity);
            Assert.Equal((ushort)0, new FixedUpdateRate().Phase);
            var e = Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry, new SetSilencer(default(FixedUpdateRate))));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, e.Code);
            Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry, new SetSilencer(new FixedUpdateRate(256, 0))));
        }

        [Fact]
        public void TheDefaultSamplingConfigIsUnset()
        {
            using var geometry = Fixture.SingleDevice();
            var unset = default(SamplingConfig);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => unset.Divide()).Code);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => unset.Freq()).Code);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => unset.Period()).Code);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => new SamplingConfig().Divide()).Code);
            Assert.NotEqual(SamplingConfig.Freq4k, unset);
            Assert.NotEqual(unset, default(SamplingConfig));
            var e = Assert.Throws<Autd3Exception>(() =>
                Frames.Encode(geometry, new ConfigModulation(ModulationBank.B0, unset, 2)));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, e.Code);

            Assert.Equal(SamplingConfig.Freq4k, new SineOption().SamplingConfig);
            Assert.Equal(SamplingConfig.Freq4k, default(SineOption).SamplingConfig);
            Assert.Equal(SamplingConfig.Freq4k, default(SquareOption).SamplingConfig);
        }

        [Fact]
        public void EncodeFailuresCarryTheRustErrorCode()
        {
            using var empty = new Geometry(Array.Empty<Autd3>());
            var encode = Assert.Throws<Autd3Exception>(() => Frames.Encode(empty, new Clear()));
            using var frames = new Frames();
            var encodeInto = Assert.Throws<Autd3Exception>(() => frames.EncodeInto(empty, new Clear()));
            Assert.Equal(encodeInto.Code, encode.Code);
            Assert.Equal(encodeInto.Message, encode.Message);
        }

        private enum Range
        {
            Near,
            Far,
            Missing,
        }

        [Fact]
        public void GroupsExposeTheirKeysAndMasks()
        {
            using var geometry = Fixture.Devices(2);
            var groups = new TransducerGroups<Range>(geometry, (device, tr) => device.Idx == 0 && tr < 10 ? Range.Near : Range.Far);
            Assert.Equal(new[] { Range.Near, Range.Far }, groups.Keys);
            Assert.Equal((Range?)Range.Near, groups.Key(0, 0));
            Assert.Equal((Range?)Range.Far, groups.Key(1, 0));
            Assert.Equal(10, groups.NumTransducersIn(Range.Near));
            Assert.Equal(10, groups.Mask(Range.Near)!.Value.NumEnabled(geometry));
            Assert.Null(groups.Mask(Range.Missing));
            Assert.Equal(0, groups.NumTransducersIn(Range.Missing));

            using var near = geometry.PhaseBuffer();
            Pattern.SetPhase(new Phase(0x10), near);
            using var far = geometry.PhaseBuffer();
            Pattern.SetPhase(new Phase(0x20), far);
            using var dst = geometry.PhaseBuffer();
            Pattern.Group(geometry, groups, key => key == Range.Near ? near : far, dst);
            Assert.Equal(new Phase(0x10), dst[0][0]);
            Assert.Equal(new Phase(0x20), dst[0][10]);
            Assert.Equal(new Phase(0x20), dst[1][0]);

            var seen = new List<Range>();
            using var intensities = geometry.IntensityBuffer();
            Pattern.GroupCompute(geometry, groups, (key, mask, _, _) =>
            {
                seen.Add(key);
                Assert.Equal(groups.NumTransducersIn(key), mask.NumEnabled(geometry));
            }, dst, intensities);
            Assert.Equal(groups.Keys, seen);
        }

        [Fact]
        public async Task BusStatsStayReadableAfterTheClientIsClosed()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            BusStats stats;
            ulong frames;
            await using (var client = await Fixture.OpenAsync(emulator, geometry))
            {
                stats = client.BusStats();
                await client.SendAsync(new Clear());
                frames = stats.Frames;
                Assert.True(frames > 0);
            }
            Assert.True(stats.Frames >= frames);
            stats.Dispose();
            Assert.Throws<ObjectDisposedException>(() => stats.Frames);
        }

        [Fact]
        public void TracingIsInitializedOncePerProcess()
        {
            Assert.Equal("info", new TracingOption().DefaultFilter);
            Assert.Equal(LogWriter.Stdout, default(TracingOption).Writer);
            Assert.Equal(LogWriter.Stderr, new TracingOption { Writer = LogWriter.Stderr }.Writer);

            var option = new TracingOption { DefaultFilter = "off", Writer = LogWriter.Stderr };
            using var guard = Tracing.Init(option);
            var e = Assert.Throws<Autd3Exception>(() => Tracing.Init(option));
            Assert.Contains("tracing subscriber", e.Message);
            Assert.Throws<Autd3Exception>(() => Tracing.Init(new TracingOption { DefaultFilter = "a\0b" }));
        }

        [Fact]
        public void TheTelemetryCountFollowsTheNativeList()
        {
            Assert.Equal(TelemetryExt.All.Count, TelemetryCounters.Count);
            Assert.Equal(Enum.GetValues(typeof(Telemetry)).Cast<Telemetry>().OrderBy(t => (byte)t), TelemetryExt.All);
        }

        [Fact]
        public void ParamsAreReadFromTheNativeLibrary()
        {
            Assert.Equal(TimeSpan.FromTicks(250), Params.UltrasoundPeriod);
            Assert.Equal(40000u, Params.UltrasoundFreqHz);
            Assert.Equal(249, Autd3.NumTransducers);
            Assert.Equal(18u, Autd3.GridX);
            Assert.Equal(14u, Autd3.GridY);
            Assert.Equal(10.16f, Autd3.PitchMm);
            Assert.Equal(Params.MaxInflight, Client.MaxInflight);
            Assert.Equal(128, Client.MaxDevices);
            Assert.Equal(256, SetPulseWidthTable.TableSize);
            Assert.True(Params.ModBufferSamples > 0);
            Assert.True(Params.NumFociMax > 0);
        }

        [Fact]
        public void ASamplingConfigErrorCarriesTheRustMessage()
        {
            var config = new SamplingConfig(4001f * Hz);
            var e = Assert.Throws<Autd3Exception>(() => config.Divide());
            Assert.Contains("must divide the ultrasound frequency", e.Message);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, e.Code);
            Assert.Throws<Autd3Exception>(() => config.Freq());
            Assert.Throws<Autd3Exception>(() => config.Period());
        }

        [Fact]
        public void SamplingConfigsAreEqualWhenTheirDividersAre()
        {
            var byFreq = new SamplingConfig(4000 * Hz);
            var byDivide = new SamplingConfig(10);
            var byPeriod = new SamplingConfig(TimeSpan.FromTicks(2500));
            Assert.Equal(byFreq, byDivide);
            Assert.True(byDivide == byPeriod);
            Assert.Equal(byFreq.GetHashCode(), byPeriod.GetHashCode());
            Assert.True(byFreq != new SamplingConfig(20));
            Assert.Equal(SamplingConfig.Freq4k, byFreq);

            var invalid = new SamplingConfig(4001f * Hz);
            Assert.False(invalid.Equals(new SamplingConfig(4001f * Hz)));

            Assert.Equal(4000f, byDivide.Freq().Hz);
            Assert.Equal(TimeSpan.FromTicks(2500), byFreq.Period());
        }

        [Theory]
        [InlineData(0.5f, 1)]
        [InlineData(1.5f, 2)]
        [InlineData(2.5f, 3)]
        [InlineData(-0.5f, 255)]
        [InlineData(255.5f, 0)]
        [InlineData(128f, 128)]
        public void AnAngleRoundsToAPhaseLikeRust(float lsb, int expected)
        {
            var angle = Angle.FromRad(lsb * (2f * MathF.PI / 256f));
            Assert.Equal((byte)expected, ((Phase)angle).Value);
        }

        [Fact]
        public void PhaseRadIsComputedWithoutTheNativeLibrary()
        {
            Assert.Equal(0f, Phase.Zero.Rad());
            Assert.Equal(MathF.PI, Phase.Pi.Rad(), 6);
            Assert.Equal(255f / 256f * 2f * MathF.PI, new Phase(255).Rad(), 6);
        }

        [Fact]
        public void ANegativeIntegerFrequencyIsRejected()
        {
            var e = Assert.Throws<Autd3Exception>(() => -1 * Hz);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, e.Code);
            Assert.Throws<Autd3Exception>(() => -1 * kHz);
            Assert.Equal(200u, (200 * Hz).HzIntValue);
            Assert.Equal(200u, Freq.FromHz(200u).HzIntValue);
            Assert.Equal(Freq.FreqMode.FloatExact, Freq.FromHz(200f).Mode);
        }

        [Fact]
        public void SysTimeArithmeticSaturates()
        {
            var max = SysTime.FromNanos(ulong.MaxValue);
            Assert.Equal(max, max + TimeSpan.FromSeconds(1));
            Assert.Equal(SysTime.Zero, SysTime.Zero - TimeSpan.FromSeconds(1));
            Assert.Equal(SysTime.FromNanos(1_000_000), SysTime.Zero + TimeSpan.FromMilliseconds(1));
            Assert.Equal(TimeSpan.FromMilliseconds(1), SysTime.FromNanos(3_000_000) - SysTime.FromNanos(2_000_000));
            Assert.Equal(TimeSpan.Zero, SysTime.FromNanos(1) - SysTime.FromNanos(2));
        }

        [Fact]
        public void OnlyLaterIsLater()
        {
            Assert.True(TransitionMode.Later.IsLater);
            Assert.False(TransitionMode.Immediate.IsLater);
            Assert.False(TransitionMode.Ext.IsLater);
        }

        [Fact]
        public void TheInterfaceIsReadable()
        {
            Assert.True(Interface.Auto.IsAuto);
            Assert.Null(Interface.Auto.NameValue);
            Assert.Equal("eth0", Interface.Name("eth0").NameValue);
            Assert.False(Interface.Name("eth0").IsAuto);
            Assert.True(Interface.Simulator.IsSimulator);
            Assert.Equal("[::1]:8080", Interface.Addr("[::1]:8080").AddrValue);
        }

        [Fact]
        public void ANonUnitRotationIsRejected()
        {
            foreach (var rotation in new[]
            {
                new Quaternion(0f, 0f, 0f, 0f),
                new Quaternion(0f, 0f, 0f, 2f),
                new Quaternion(0f, 0f, 0f, 1.002f),
            })
            {
                var e = Assert.Throws<Autd3Exception>(() => new Geometry(new[] { new Autd3(Vector3.Zero, rotation) }));
                Assert.Contains("unit quaternion", e.Message);
                Assert.Equal(Autd3ErrorCode.InvalidArgument, e.Code);
            }
            using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero, new Quaternion(0f, 0f, 0f, 1.0005f)) });
            Assert.Equal(1, geometry.NumDevices);
        }

        [Fact]
        public void ADeviceConvertsAPointToItsLocalFrame()
        {
            using var geometry = new Geometry(new[] { new Autd3(new Vector3(10f, 20f, 30f)) });
            var local = geometry[0].ToLocal(new Vector3(11f, 22f, 33f));
            Assert.Equal(1f, local.X, 4);
            Assert.Equal(2f, local.Y, 4);
            Assert.Equal(3f, local.Z, 4);
        }

        [Fact]
        public void PositionsMatchTheSingleTransducerGetter()
        {
            using var geometry = new Geometry(new[] { new Autd3(new Vector3(10f, 20f, 30f)) });
            var device = geometry[0];
            var positions = device.Positions;
            Assert.Equal(device.NumTransducers, positions.Length);
            for (var tr = 0; tr < positions.Length; tr++)
            {
                Assert.Equal(device.Position(tr), positions[tr]);
            }
        }

        [Fact]
        public void PointAndOffsetTakeLengths()
        {
            Assert.Equal(new Vector3(1f, 2000f, 3f), Point(1 * mm, 2 * m, 3 * mm));
            Assert.Equal(new Vector3(-30f, 0f, 150f), Offset(-30 * mm, 0 * mm, 150 * mm));
        }

        [Fact]
        public void ATransducerMaskReportsItsShape()
        {
            using var geometry = Fixture.Devices(2);
            Assert.True(TransducerMask.AllEnabled.IsEnabled(1, 3));
            Assert.Equal(geometry.NumTransducers, TransducerMask.AllEnabled.NumEnabled(geometry));
            TransducerMask.AllEnabled.Validate(geometry);

            var flags = new[] { new bool[Autd3.NumTransducers], new bool[Autd3.NumTransducers] };
            flags[1][3] = true;
            var mask = TransducerMask.Masked(flags);
            mask.Validate(geometry);
            Assert.True(mask.IsEnabled(1, 3));
            Assert.False(mask.IsEnabled(0, 3));
            Assert.Equal(1, mask.NumEnabled(geometry));

            var e = Assert.Throws<Autd3Exception>(() => TransducerMask.Masked(new[] { new bool[Autd3.NumTransducers] }).Validate(geometry));
            Assert.Contains("the mask has 1 device slots but the geometry has 2 devices", e.Message);
            e = Assert.Throws<Autd3Exception>(() => TransducerMask.Masked(new[] { new bool[Autd3.NumTransducers], new bool[3] }).Validate(geometry));
            Assert.Contains("the mask slot for device 1 has 3 transducers", e.Message);
        }

        [Fact]
        public void TransducerGroupsExposeTheirIndices()
        {
            using var geometry = Fixture.Devices(2);
            var groups = new TransducerGroups<int>(geometry, (device, tr) => device.Idx == 1 && tr < 5 ? 7 : 3);
            Assert.Equal(new[] { 3, 7 }, groups.Keys);
            Assert.Equal(2, groups.NumDevices);
            Assert.Equal(Autd3.NumTransducers, groups.NumTransducers(1));
            Assert.Equal(5, groups.NumTransducersIn(7));
            Assert.Equal(geometry.NumTransducers - 5, groups.NumTransducersIn(3));
            Assert.Equal(0, groups.NumTransducersIn(99));
            Assert.Equal(1, groups.Index(1, 0));
            Assert.Equal(0, groups.Index(1, 5));
            Assert.All(groups.Indices(0), index => Assert.Equal(0, index));
            Assert.Equal(Autd3.NumTransducers, groups.Indices(1).Count);
            Assert.Equal(5, groups.Indices(1).Count(index => index == 1));

            var masks = groups.Masks().ToArray();
            Assert.Equal(new[] { 3, 7 }, masks.Select(m => m.Key));
            Assert.Equal(5, masks[1].Mask.NumEnabled(geometry));
            Assert.True(masks[1].Mask.IsEnabled(1, 4));
            Assert.False(masks[1].Mask.IsEnabled(1, 5));
        }

        [Fact]
        public void BuffersCopyOutAsArrays()
        {
            using var geometry = Fixture.Devices(2);
            using var phases = geometry.PhaseBuffer();
            var devicePhases = phases[1];
            devicePhases[2] = new Phase(0x40);
            var phaseArray = phases.ToArray();
            Assert.Equal(2, phaseArray.Length);
            Assert.Equal(Autd3.NumTransducers, phaseArray[1].Length);
            Assert.Equal((byte)0x40, phaseArray[1][2].Value);
            Assert.Equal(1, phaseArray.Sum(device => device.Count(p => p.Value != 0)));

            using var intensities = geometry.IntensityBuffer();
            var deviceIntensities = intensities[0];
            deviceIntensities[1] = new Intensity(0x80);
            Assert.Equal((byte)0x80, intensities.ToArray()[0][1].Value);

            using var modulation = ModulationBuffer.FromBytes(new byte[] { 1, 2, 3 });
            Assert.Equal(new byte[] { 1, 2, 3 }, modulation.ToArray());
        }

        [Fact]
        public void SamplesPerPeriodIsNullWhenItIsNotAnInteger()
        {
            Assert.Equal(20u, Modulation.SamplesPerPeriod(10, 200 * Hz));
            Assert.Null(Modulation.SamplesPerPeriod(10, 3 * Hz));
            var e = Assert.Throws<Autd3Exception>(() => Modulation.SamplesPerPeriod(10, 200f * Hz));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, e.Code);
        }

        [Fact]
        public void FpgaStateBitsFollowTheNativeGetters()
        {
            var idle = new FpgaState(0);
            Assert.Equal(ModulationBank.B0, idle.CurrentModBank);
            Assert.Equal(PatternBank.B0, idle.CurrentPatternBank);
            Assert.False(idle.IsPatternMode);
            Assert.True(idle.IsStmMode);

            Assert.True(new FpgaState(1 << 0).IsThermalAsserted);
            Assert.Equal(ModulationBank.B1, new FpgaState(1 << 1).CurrentModBank);
            Assert.Equal(PatternBank.B1, new FpgaState(1 << 2).CurrentPatternBank);
            Assert.True(new FpgaState(1 << 3).IsPatternMode);
            Assert.False(new FpgaState(1 << 3).IsStmMode);
            Assert.True(new FpgaState(1 << 4).IsPatternStopped);
            Assert.True(new FpgaState(1 << 5).IsModStopped);
            Assert.True(new FpgaState(1 << 6).IsTransitionPending);
            Assert.True(new FpgaState(1 << 7).IsFailsafeActive);
        }

        [Fact]
        public void FramesAreReEncodedInPlace()
        {
            using var geometry = Fixture.SingleDevice();
            using var frames = new Frames();
            Assert.Equal(0, frames.Length);
            frames.EncodeInto(geometry, new Clear());
            Assert.Equal(1, frames.Length);
            frames.EncodeInto(geometry, Command.Sequence(new Clear(), new Synchronize()));
            Assert.Equal(2, frames.Length);

            using var mismatched = Fixture.Devices(2);
            var e = Assert.Throws<Autd3Exception>(() =>
                frames.EncodeInto(geometry, new SetOutputMask(new[] { new bool[1] })));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, e.Code);
        }

        [Fact]
        public void ABatchMatchesSolvingEachProblemAlone()
        {
            using var geometry = Fixture.SingleDevice();
            var wavelength = Pattern.Wavelength(340 * m / s);
            var foci = new[] { -30f, 30f, -10f, 10f }
                .Select(x => new AmplitudeTarget(geometry.Center + new Vector3(x, 0f, 150f), 2.5e3f * Pa))
                .ToArray();

            using var p0 = geometry.PhaseBuffer();
            using var p1 = geometry.PhaseBuffer();
            using var i0 = geometry.IntensityBuffer();
            using var i1 = geometry.IntensityBuffer();
            Holo.Holo.GspatBatch(geometry, foci, wavelength, new GspatOption(), new[] { p0, p1 }, new[] { i0, i1 });

            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            Holo.Holo.Gspat(geometry, foci.Skip(2).ToArray(), wavelength, new GspatOption(), phases, intensities);
            Assert.Equal(phases.ToArray()[0].Select(p => p.Value), p1.ToArray()[0].Select(p => p.Value));
            Assert.Equal(intensities.ToArray()[0].Select(i => i.Value), i1.ToArray()[0].Select(i => i.Value));

            Holo.Holo.NaiveBatch(geometry, foci, wavelength, new NaiveOption(), new[] { p0, p1 }, new[] { i0, i1 });
            Holo.Holo.GsBatch(geometry, foci, wavelength, new GsOption(), new[] { p0, p1 }, new[] { i0, i1 });

            var e = Assert.Throws<Autd3Exception>(() =>
                Holo.Holo.GspatBatch(geometry, foci, wavelength, new GspatOption(), new[] { p0, p0 }, new[] { i0, i1 }));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, e.Code);
            Assert.Throws<Autd3Exception>(() =>
                Holo.Holo.GspatBatch(geometry, foci, wavelength, new GspatOption(), new[] { p0 }, new[] { i0, i1 }));
            Assert.Equal(1, p0.NumDevices);
        }

        [Fact]
        public async Task TheClientReportsVersionsStatsAndValues()
        {
            using var geometry = Fixture.Devices(2);
            using var emulator = new UdpEmulator(2);
            await using var client = await Fixture.OpenAsync(emulator, geometry);

            var versions = await client.ReadFirmwareVersionAsync();
            Assert.Equal(2, versions.Count);
            var (major, minor) = FirmwareVersion.SupportedSeries;
            foreach (var version in versions)
            {
                Assert.True(version.IsEmulator);
                Assert.True(version.IsSupported);
                Assert.Equal(major, version.Cpu.Major);
                Assert.Equal(minor, version.Cpu.Minor);
                Assert.False(version.Fpga.IsUnknown);
                Assert.Equal($"CPU: {version.Cpu}, FPGA: {version.Fpga} [Emulator]", version.ToString());
            }

            using var stats = client.BusStats();
            var framesBefore = stats.Frames;
            var ackedBefore = stats.AckedFrames;
            await client.SendAsync(new Clear());
            Assert.True(stats.Frames > framesBefore);
            Assert.True(stats.AckedFrames > ackedBefore);
            Assert.True(stats.WorstAckLatencyNs >= stats.MeanAckLatencyNs);
            Assert.Contains($"Frames = {stats.Frames}", stats.ToString());

            using var frames = Frames.Encode(client.Geometry, new Clear());
            var response = await await client.SendFrameAsync(frames[0]);
            Assert.Equal(2, response.Status.Count);
            Assert.Equal(2, response.Values.Count);
            Assert.Equal(response.Values[1], response.Value(1));
            Assert.Empty(response.Value(5));
            response.Check();
        }

        [Fact]
        public async Task ADeviceErrorCarriesItsCode()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            await client.SendAsync(new SetSilencer(new FixedCompletionTime { Intensity = TimeSpan.FromTicks(5000), Phase = TimeSpan.FromTicks(10000) }));
            using var modulation = ModulationBuffer.FromBytes(new byte[] { 0xFF, 0xFF });

            var e = await Assert.ThrowsAsync<Autd3Exception>(() =>
                client.SendAsync(new Modulation(new SamplingConfig(1), modulation)));
            Assert.Equal(Autd3ErrorCode.Device, e.Code);

            using var frames = Frames.Encode(geometry, new Modulation(new SamplingConfig(1), modulation));
            Response? rejected = null;
            foreach (var frame in frames)
            {
                var response = await await client.SendFrameAsync(frame);
                if (response.Status.Any(status => status != 0))
                {
                    rejected = response;
                }
            }
            Assert.NotNull(rejected);
            e = Assert.Throws<Autd3Exception>(() => rejected!.Check());
            Assert.Equal(Autd3ErrorCode.Device, e.Code);
        }

        [Fact]
        public async Task TheClientKeepsItsOwnGeometry()
        {
            var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            Assert.NotSame(geometry, client.Geometry);
            geometry.Dispose();
            Assert.Equal(1, client.Geometry.NumDevices);
            await client.SendAsync(new Clear());
        }

        [Fact]
        public async Task ALostChainReportsANetworkCode()
        {
            using var geometry = Fixture.SingleDevice();
            var emulator = new UdpEmulator(1);
            var client = await Fixture.OpenAsync(emulator, geometry);
            using var checker = client.StateChecker();
            await client.DisposeAsync();
            emulator.Dispose();
            var e = Assert.Throws<Autd3Exception>(() => checker.Check());
            Assert.NotEqual(0, (int)e.Code);
        }
    }
}
