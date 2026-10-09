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

        private static async Task<uint> CountAsync(Client client, Telemetry counter, int device = 0) =>
            (await client.ReadTelemetryAsync())[device][counter];

        [Fact]
        public async Task SendAsyncWaitsForEveryFrame()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            var command = new Pattern(phases, intensities);
            using var frames = Frames.Encode(geometry, command);

            var before = await CountAsync(client, Telemetry.Processed);
            var afterARead = await CountAsync(client, Telemetry.Processed);
            await client.SendAsync(command);
            var after = await CountAsync(client, Telemetry.Processed);

            Assert.Equal((uint)frames.Length, after - afterARead - (afterARead - before));
            Assert.Equal(0u, await CountAsync(client, Telemetry.DispatchError));
        }

        [Fact]
        public async Task SendAsyncStopsAtTheFrameTheDeviceRejects()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            await PlayAModulationTheStrictSilencerRejects(client);

            await Assert.ThrowsAsync<Autd3Exception>(() => client.SendAsync(
                Command.Sequence(RejectedByTheDevice(), RejectedByTheDevice(), RejectedByTheDevice())));

            Assert.Equal(1u, await CountAsync(client, Telemetry.DispatchError));
        }

        [Fact]
        public async Task SendAsyncSendsNothingWhenTheCommandFailsToEncode()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);

            var before = await CountAsync(client, Telemetry.Processed);
            var afterARead = await CountAsync(client, Telemetry.Processed);
            await Assert.ThrowsAsync<Autd3Exception>(() => client.SendAsync(Command.Sequence(
                new Nop(),
                new ActivateModulationBank(ModulationBank.B1, TransitionMode.Later))));
            await Assert.ThrowsAsync<Autd3Exception>(async () => await client.SendStreamingAsync(Command.Sequence(
                new Nop(),
                new ActivateModulationBank(ModulationBank.B1, TransitionMode.Later))));
            var after = await CountAsync(client, Telemetry.Processed);

            Assert.Equal(afterARead - before, after - afterARead);
        }

        [Fact]
        public async Task SendStreamingAsyncCompletesInTwoStages()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            var points = new List<ControlPoints>();
            Stm.Circle(geometry.Center + new Vector3(0f, 0f, 150f), 30f * mm, 64, new Vector3(0f, 0f, 1f), Intensity.Max, points);
            var command = new FociStm(1 * Hz, points.ToArray());
            using var frames = Frames.Encode(geometry, command);

            var before = await CountAsync(client, Telemetry.Processed);
            var afterARead = await CountAsync(client, Telemetry.Processed);
            StreamFuture queued = await client.SendStreamingAsync(command);
            await queued;
            var after = await CountAsync(client, Telemetry.Processed);

            Assert.Equal((uint)frames.Length, after - afterARead - (afterARead - before));
            await Assert.ThrowsAsync<Autd3Exception>(async () => await queued);
        }

        [Fact]
        public async Task SendStreamingAsyncReportsTheFirstErrorAfterQueueingEveryFrame()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            await PlayAModulationTheStrictSilencerRejects(client);

            var queued = await client.SendStreamingAsync(
                Command.Sequence(RejectedByTheDevice(), RejectedByTheDevice(), RejectedByTheDevice()));
            await Assert.ThrowsAsync<Autd3Exception>(async () => await queued);

            Assert.Equal(3u, await CountAsync(client, Telemetry.DispatchError));
        }

        [Fact]
        public async Task SendStreamingAsyncReportsTheErrorOfTheEarliestFrame()
        {
            using var geometry = Fixture.Devices(2);
            using var emulator = new UdpEmulator(2);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            await PlayAModulationTheStrictSilencerRejects(client);

            var queued = await client.SendStreamingAsync(Command.Sequence(
                Command.Each(device => device.Idx == 1 ? RejectedByTheDevice() : null),
                Command.Each(device => device.Idx == 0 ? RejectedByTheDevice() : null)));
            var e = await Assert.ThrowsAsync<Autd3Exception>(async () => await queued);

            Assert.Contains("device 1", e.Message);
        }

        [Fact]
        public async Task SendStreamingAsyncAfterTheClientIsDisposedThrows()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            var client = await Fixture.OpenAsync(emulator, geometry);
            await client.DisposeAsync();

            await Assert.ThrowsAsync<ObjectDisposedException>(async () => await client.SendStreamingAsync(new Nop()));
        }

        [Fact]
        public async Task ADroppedStreamFutureDoesNotStopTheFrames()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);

            var before = await CountAsync(client, Telemetry.Processed);
            var afterARead = await CountAsync(client, Telemetry.Processed);
            (await client.SendStreamingAsync(Command.Sequence(new Nop(), new Nop()))).Dispose();
            await client.SendAsync(new Nop());
            var after = await CountAsync(client, Telemetry.Processed);

            Assert.Equal(3u, after - afterARead - (afterARead - before));
        }

        [Fact]
        public async Task EncodedFramesAreSentOneByOne()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();
            using var frames = Frames.Encode(client.Geometry, new Pattern(phases, intensities));
            Assert.Equal(3, frames.Length);

            for (var round = 0; round < 2; round++)
            {
                foreach (var frame in frames)
                {
                    ResponseFuture token = await client.SendFrameAsync(frame);
                    var response = await token;
                    Assert.Equal(client.NumDevices, response.Status.Count);
                    response.Check();
                }
            }
        }

        [Fact]
        public async Task SendFrameAsyncLeavesTheStatusToTheCaller()
        {
            using var geometry = Fixture.SingleDevice();
            using var emulator = new UdpEmulator(1);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            await PlayAModulationTheStrictSilencerRejects(client);
            using var frames = Frames.Encode(geometry, RejectedByTheDevice());

            var response = await await client.SendFrameAsync(frames[0]);

            Assert.NotEqual(0, response.Status[0]);
            Assert.Throws<Autd3Exception>(() => response.Check());
        }

        [Fact]
        public async Task EachSendsTheCommandOnlyToItsDevice()
        {
            using var geometry = Fixture.Devices(2);
            using var emulator = new UdpEmulator(2);
            await using var client = await Fixture.OpenAsync(emulator, geometry);
            await PlayAModulationTheStrictSilencerRejects(client);

            await Assert.ThrowsAsync<Autd3Exception>(() => client.SendAsync(
                Command.Each(device => device.Idx == 1 ? RejectedByTheDevice() : null)));

            Assert.Equal(0u, await CountAsync(client, Telemetry.DispatchError, 0));
            Assert.Equal(1u, await CountAsync(client, Telemetry.DispatchError, 1));
        }

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
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();

            using var frames = Frames.Encode(geometry, Command.Each(device =>
                device.Idx == 0 ? new Pattern(phases, intensities) : (ICommand)new Nop()));

            Assert.Equal(3, frames.Length);
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

            Assert.Equal(7, frames.Length);
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
