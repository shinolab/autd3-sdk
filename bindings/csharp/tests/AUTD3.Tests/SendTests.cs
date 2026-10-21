using System;
using System.Collections.Generic;
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
        public void EachSpansTheLongestPerDeviceCommand()
        {
            using var geometry = Fixture.Devices(2);

            using var frames = Frames.Encode(geometry, Command.Each(device =>
                device.Idx == 0 ? Command.Sequence(new Nop(), new Nop()) : (ICommand)new Nop()));

            Assert.Equal(2, frames.Length);
        }

        [Fact]
        public void EachNests()
        {
            using var geometry = Fixture.Devices(2);

            using var frames = Frames.Encode(geometry, Command.Each(outer =>
                outer.Idx == 1
                    ? Command.Each(inner => inner.Idx == 1 ? Command.Sequence(new Nop(), new Nop()) : null)
                    : null));

            Assert.Equal(2, frames.Length);
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
