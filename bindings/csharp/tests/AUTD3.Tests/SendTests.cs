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
        public void EachTakesMultiFrameCommandsAndOneInstanceMayServeSeveralDevices()
        {
            using var geometry = Fixture.Devices(3);
            using var modulation = ModulationBuffer.FromBytes(new byte[] { 0xFF, 0xFF });
            var shared = new Modulation(SamplingConfig.Freq4k, modulation);
            using var alone = Frames.Encode(geometry, shared);

            using var frames = Frames.Encode(geometry, Command.Each(device =>
                device.Idx == 1 ? new Clear() : (ICommand)shared));
            using var nested = Frames.Encode(geometry, Command.Each(device =>
                device.Idx == 0 ? Command.Each(_ => new Nop()) : null));
            using var none = Frames.Encode(geometry, Command.Each(_ => null));

            Assert.Equal(alone.Length, frames.Length);
            Assert.Equal(1, nested.Length);
            Assert.Equal(0, none.Length);
        }

        [Fact]
        public void AFailingEachFactoryFreesTheHandlesItAlreadyCreated()
        {
            using var geometry = Fixture.Devices(3);
            using var modulation = ModulationBuffer.FromBytes(new byte[] { 0xFF, 0xFF });
            var shared = new Modulation(SamplingConfig.Freq4k, modulation);

            Assert.Throws<InvalidOperationException>(() => Frames.Encode(geometry, Command.Each(device =>
                device.Idx < 2 ? shared : throw new InvalidOperationException())));
        }

        [Fact]
        public void EachTakesTheBufferWrites()
        {
            using var geometry = Fixture.Devices(2);
            using var modulation = ModulationBuffer.FromBytes(new byte[] { 0xFF, 0xFF });
            var points = new[] { new ControlPoints(new[] { new ControlPoint(new Vector3(0f, 0f, 150f), Phase.Zero) }, Intensity.Max) };

            using var modulationFrames = Frames.Encode(geometry,
                Command.Each(_ => new WriteModulationBuffer(ModulationBank.B0, 0, modulation)));
            using var fociFrames = Frames.Encode(geometry,
                Command.Each(device => device.Idx == 1 ? new WriteFociBuffer(PatternBank.B0, 0, points) : (ICommand?)null));

            Assert.Equal(1, modulationFrames.Length);
            Assert.Equal(1, fociFrames.Length);
        }

        [Fact]
        public void BufferWritesLongerThanOneFrameAreSplitIntoFrames()
        {
            using var geometry = Fixture.Devices(1);
            using var modulation = ModulationBuffer.FromBytes(new byte[4000]);

            using (var frames = Frames.Encode(geometry, new WriteModulationBuffer(ModulationBank.B0, 0, modulation)))
            {
                Assert.True(frames.Length > 1);
            }

            var point = new ControlPoint(new Vector3(0f, 0f, 150f), Phase.Zero);
            var points = Enumerable.Range(0, 200)
                .Select(_ => new ControlPoints(new[] { point, point }, Intensity.Max))
                .ToArray();
            using (var frames = Frames.Encode(geometry, new WriteFociBuffer(PatternBank.B0, 0, points)))
            {
                Assert.True(frames.Length > 1);
            }
        }

        [Fact]
        public void AFailingEachFactoryPropagatesItsException()
        {
            using var geometry = Fixture.Devices(2);

            Assert.Throws<InvalidOperationException>(() => Frames.Encode(geometry, Command.Each(device =>
                device.Idx == 0 ? new Nop() : throw new InvalidOperationException())));
        }

        [Fact]
        public void NullArgumentsAreRejectedBeforeReachingTheNativeLayer()
        {
            using var geometry = Fixture.SingleDevice();

            Assert.Throws<ArgumentNullException>(() => Command.Each(null!));
            Assert.Throws<ArgumentNullException>(() => Frames.Encode(geometry, (ICommand)null!));
            Assert.Throws<ArgumentNullException>(() => Frames.Encode(geometry, (IEnumerable<ICommand>)null!));
            Assert.Throws<ArgumentNullException>(() => Frames.Encode(geometry, new List<ICommand> { new Nop(), null! }));
            Assert.Throws<ArgumentNullException>(() => Frames.Encode(null!, new Nop()));
        }

        [Fact]
        public void AListOfCommandsExpandsInOrder()
        {
            using var geometry = Fixture.Devices(2);
            using var phases = geometry.PhaseBuffer();
            using var intensities = geometry.IntensityBuffer();

            using var frames = Frames.Encode(geometry, new List<ICommand>
            {
                new Clear(),
                Command.Each(device => device.Idx == 0 ? new Nop() : null),
                Command.Each(device => device.Idx == 1 ? new Nop() : null),
                new Pattern(phases, intensities),
                new Nop(),
            });

            Assert.Equal(5, frames.Length);
        }

        [Fact]
        public void AnEmptyListEncodesToNoFrames()
        {
            using var geometry = Fixture.SingleDevice();
            using var frames = Frames.Encode(geometry, new List<ICommand>());
            Assert.Equal(0, frames.Length);
        }

        [Fact]
        public void AListIsEncodedIntoExistingFramesInPlace()
        {
            using var geometry = Fixture.SingleDevice();
            using var frames = Frames.Encode(geometry, new Nop());

            frames.EncodeInto(geometry, new ICommand[] { new Nop(), new Synchronize() });

            Assert.Equal(2, frames.Length);
        }

        [Fact]
        public void AFailingMemberFailsTheWholeList()
        {
            using var geometry = Fixture.SingleDevice();

            Assert.Throws<Autd3Exception>(() => Frames.Encode(geometry, new List<ICommand>
            {
                new Nop(),
                new SetCpuConfig(new CpuConfig { Ptp = new PtpConfig { LockSamples = 0 } }),
            }));
        }
    }
}
