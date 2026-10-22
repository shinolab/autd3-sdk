using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using System.Threading.Tasks;
using Xunit;
using static AUTD3.Units;

namespace AUTD3.Tests
{
    public class SendTests
    {
        private static ICommand RejectedByTheDevice() =>
            new SetSilencer(new FixedCompletionTime { Intensity = TimeSpan.FromTicks(5000), Phase = TimeSpan.FromTicks(10000) });

        private static async Task PlayAModulationTheStrictSilencerRejects(Client client)
        {
            using var modulation = ModulationBuffer.FromBytes(new byte[] { 0xFF, 0xFF });
            await client.SendAsync(new Modulation(SamplingConfig.Freq4k, modulation));
        }

        private static async Task<uint> ProcessedAsync(Client client) =>
            (await client.ReadTelemetryAsync())[0].Processed;

        private static async Task<uint> DispatchErrorAsync(Client client, int device = 0) =>
            (await client.ReadTelemetryAsync())[device].DispatchError;

        [Fact]
        public void EachVisitsEveryDeviceWhenEncoded()
        {
            using var geometry = Fixture.Devices(3);
            var visited = new List<int>();
            var command = Command.Each(device =>
            {
                visited.Add(device.Idx);
                return null;
            });
            Assert.Empty(visited);

            using var frames = Frames.Encode(geometry, command);

            Assert.Equal(new[] { 0, 1, 2 }, visited);
            Assert.Equal(0, frames.Length);
        }

        [Fact]
        public void EachTakesOneFrameAndFillsTheUnassignedDevicesWithNop()
        {
            using var geometry = Fixture.Devices(2);

            using var frames = Frames.Encode(geometry, Command.Each(device =>
                device.Idx == 0 ? new Clear() : (ICommand?)null));

            Assert.Equal(1, frames.Length);
        }

        [Fact]
        public void EachRejectsWhatIsNotASingleFrameOperation()
        {
            using var geometry = Fixture.Devices(2);
            using var modulation = ModulationBuffer.FromBytes(new byte[] { 0xFF, 0xFF });
            var notSingleFrame = new ICommand[]
            {
                new Modulation(SamplingConfig.Freq4k, modulation),
                Command.Sequence(new Nop()),
                Command.Each(_ => new Nop()),
            };

            foreach (var command in notSingleFrame)
            {
                var error = Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry, Command.Each(device =>
                    device.Idx == 1 ? command : null)));
                Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            }
        }

        [Fact]
        public void EachTakesTheBufferWritesAsSingleFrameOperations()
        {
            using var geometry = Fixture.Devices(2);
            using var modulation = ModulationBuffer.FromBytes(new byte[] { 0xFF, 0xFF });
            var points = new[] { new ControlPoints(new[] { new ControlPoint(new Vector3(0f, 0f, 150f), Phase.Zero) }, Intensity.Max) };

            using var frames = Frames.Encode(geometry, Command.Sequence(
                Command.Each(_ => new WriteModulationBuffer(ModulationBank.B0, 0, modulation)),
                Command.Each(device => device.Idx == 1 ? new WriteFociBuffer(PatternBank.B0, 0, points) : (ICommand?)null)));

            Assert.Equal(2, frames.Length);
        }

        [Fact]
        public void BufferWritesLongerThanOneFrameAreRejectedWhenEncoded()
        {
            using var geometry = Fixture.Devices(1);
            using var fits = ModulationBuffer.FromBytes(new byte[WriteModulationBuffer.MaxLen]);
            using var tooLong = ModulationBuffer.FromBytes(new byte[WriteModulationBuffer.MaxLen + 1]);

            using (var frames = Frames.Encode(geometry, new WriteModulationBuffer(ModulationBank.B0, 0, fits)))
            {
                Assert.Equal(1, frames.Length);
            }
            var error = Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry, new WriteModulationBuffer(ModulationBank.B0, 0, tooLong)));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);

            var point = new ControlPoint(new Vector3(0f, 0f, 150f), Phase.Zero);
            var points = Enumerable.Range(0, WriteFociBuffer.MaxLen(2) + 1)
                .Select(_ => new ControlPoints(new[] { point, point }, Intensity.Max))
                .ToArray();
            using (var frames = Frames.Encode(geometry, new WriteFociBuffer(PatternBank.B0, 0, points[..^1])))
            {
                Assert.Equal(1, frames.Length);
            }
            error = Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry, new WriteFociBuffer(PatternBank.B0, 0, points)));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
        }

        [Fact]
        public void TheWriteFociBufferMaxLenShrinksWithTheNumberOfFoci()
        {
            Assert.True(WriteFociBuffer.MaxLen(1) > WriteFociBuffer.MaxLen(8));
            Assert.True(WriteFociBuffer.MaxLen(8) > 0);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => WriteFociBuffer.MaxLen(0)).Code);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, Assert.Throws<Autd3Exception>(() => WriteFociBuffer.MaxLen(9)).Code);
        }

        [Fact]
        public void AFailingEachFactoryPropagatesItsException()
        {
            using var geometry = Fixture.Devices(2);

            Assert.Throws<InvalidOperationException>(() => Frames.Encode(geometry, Command.Each(device =>
                device.Idx == 0 ? new Nop() : throw new InvalidOperationException())));
        }

        [Fact]
        public void SequenceExpandsItsCommandsInOrder()
        {
            using var geometry = Fixture.Devices(2);
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();

            using var frames = Frames.Encode(geometry, Command.Sequence(
                new Clear(),
                Command.Each(device => device.Idx == 0 ? new Nop() : null),
                Command.Each(device => device.Idx == 1 ? new Nop() : null),
                Command.Sequence(new Pattern(phases, intensities), new Nop())));

            Assert.Equal(5, frames.Length);
        }

        [Fact]
        public void AnEmptySequenceEncodesToNoFrames()
        {
            using var geometry = Fixture.SingleDevice();
            using var frames = Frames.Encode(geometry, Command.Sequence());
            Assert.Equal(0, frames.Length);
        }

        [Fact]
        public void ASequenceCanBeEncodedMoreThanOnce()
        {
            using var geometry = Fixture.SingleDevice();
            var command = Command.Sequence(new Nop(), new Synchronize());

            using var first = Frames.Encode(geometry, command);
            using var second = Frames.Encode(geometry, command);

            Assert.Equal(2, first.Length);
            Assert.Equal(2, second.Length);
        }

        [Fact]
        public void NullArgumentsAreRejectedBeforeReachingTheNativeLayer()
        {
            using var geometry = Fixture.SingleDevice();

            Assert.Throws<ArgumentNullException>(() => Command.Each(null!));
            Assert.Throws<ArgumentNullException>(() => Command.Sequence(null!));
            Assert.Throws<ArgumentNullException>(() => Command.Sequence(new Nop(), null!));
            Assert.Throws<ArgumentNullException>(() => Frames.Encode(geometry, null!));
            Assert.Throws<ArgumentNullException>(() => Frames.Encode(null!, new Nop()));
        }

        [Fact]
        public void AFailingMemberFailsTheWholeSequence()
        {
            using var geometry = Fixture.SingleDevice();

            Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry, Command.Sequence(
                new Nop(),
                new SetCpuConfig(new CpuConfig { FpgaWaitUpdateMaxPolls = 0 }))));
        }
    }
}
