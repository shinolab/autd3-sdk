using System.Numerics;
using System.Threading.Tasks;
using AUTD3.Legacy;
using Xunit;

namespace AUTD3.Tests
{
    public class RtPriorityTests
    {
        private static Geometry SingleDevice() => new Geometry(new[] { new Autd3(Vector3.Zero) });

        public static TheoryData<RtPriority> AcceptedPriorities => new TheoryData<RtPriority>
        {
            RtPriority.Default,
            RtPriority.Disabled,
            RtPriority.Min,
            RtPriority.Max,
            new RtPriority(49),
        };

        [Fact]
        public void DefaultIsThePlatformDefault()
        {
            Assert.Equal(RtPriority.Default, new ClientConfig().RtPriority);
            Assert.Equal(RtPriority.Default, new LegacyClientConfig().RtPriority);
            Assert.Equal(RtPriority.Default, default(RtPriority));
            Assert.NotEqual(RtPriority.Default, RtPriority.Disabled);
            Assert.Equal(new RtPriority(49), new RtPriority(49));
        }

        [Theory]
        [MemberData(nameof(AcceptedPriorities))]
        public async Task AcceptedPrioritiesOpenTheClient(RtPriority priority)
        {
            using var geometry = SingleDevice();
            using var client = await Client.OpenAsync(geometry, new AUTD3.Link.Nop(), new ClientConfig(rtPriority: priority));
            await client.CloseAsync();
        }

        [Fact]
        public async Task AnOutOfRangePriorityIsRejected()
        {
            using var geometry = SingleDevice();
            await Assert.ThrowsAsync<Autd3Exception>(async () =>
                await Client.OpenAsync(geometry, new AUTD3.Link.Nop(), new ClientConfig(rtPriority: new RtPriority(100))));
        }
    }
}
