using System.Numerics;
using System.Threading.Tasks;
using Xunit;

namespace AUTD3.Tests
{
    public class RtPriorityTests
    {
        private static Geometry SingleDevice() => new Geometry(new[] { new Autd3(Vector3.Zero) });

        public static TheoryData<RtPriority?> AcceptedPriorities => new TheoryData<RtPriority?>
        {
            RtPriority.Default,
            null,
            RtPriority.Min,
            RtPriority.Max,
            new RtPriority(49),
        };

        [Fact]
        public void DefaultIsThePlatformDefault()
        {
            Assert.Equal(RtPriority.Default, new ClientConfig().RtPriority);
            Assert.Equal(RtPriority.Default, default(RtPriority));
            Assert.Equal(new RtPriority(49), new RtPriority(49));
        }

        [Fact]
        public void NullDisablesThePriority()
        {
            Assert.Equal(((byte)1, (byte)0), RtPriority.ToNative(null));
            Assert.Equal(((byte)0, (byte)0), RtPriority.ToNative(RtPriority.Default));
            Assert.Equal(((byte)2, (byte)49), RtPriority.ToNative(new RtPriority(49)));
            Assert.Null(new ClientConfig { RtPriority = null }.RtPriority);
        }

        [Theory]
        [MemberData(nameof(AcceptedPriorities))]
        public async Task AcceptedPrioritiesOpenTheClient(RtPriority? priority)
        {
            using var geometry = SingleDevice();
            using var emulator = new UdpEmulator(1);
            using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig { RtPriority = priority });
            await client.CloseAsync();
        }

        [Fact]
        public async Task AnOutOfRangePriorityIsRejected()
        {
            using var geometry = SingleDevice();
            using var emulator = new UdpEmulator(1);
            await Assert.ThrowsAsync<Autd3Exception>(async () =>
                await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig { RtPriority = new RtPriority(100) }));
        }
    }
}
