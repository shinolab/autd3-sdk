using System;
using System.Linq;
using System.Numerics;
using System.Threading.Tasks;
using Xunit;

namespace AUTD3.Tests
{
    public class UdpTests
    {
        private static Geometry Devices(int n) =>
            new Geometry(Enumerable.Range(0, n).Select(_ => new Autd3(Vector3.Zero)).ToArray());

        [Fact]
        public void TheNativeDefaultsFollowTheSpec()
        {
            var option = TransportOption.Defaults();
            Assert.Null(option.Group);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.Cycle);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.ReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(200), option.ResponseTimeout);
            Assert.Equal(TimeSpan.FromSeconds(10), option.EnumerationTimeout);
            Assert.Equal(TimeSpan.FromSeconds(5), option.SyncTimeout);
        }

        [Fact]
        public void AnUnsetFieldKeepsTheNativeDefault()
        {
            var option = new TransportOption();
            Assert.Equal(Interface.Auto, option.Iface);
            Assert.Null(option.Group);
            Assert.Null(option.Cycle);
            Assert.Null(option.SyncTimeout);
        }

        [Fact]
        public async Task TheClientOpensTheEmulatedChain()
        {
            using var emulator = new UdpEmulator(2);
            var option = emulator.Option();
            Assert.NotNull(option.Group);
            using var geometry = Devices(2);
            var (client, checker) = await Client.OpenWithCheckerAsync(geometry, option, new ClientConfig());
            await using (client)
            {
                Assert.Equal(2, client.NumDevices);
                Assert.Equal(2, (await client.ReadFirmwareVersionAsync()).Count);
                Assert.True(checker.Check().AllOp);
            }
        }

        [Fact]
        public async Task ADeviceCountMismatchFailsToOpen()
        {
            using var emulator = new UdpEmulator(1);
            using var geometry = Devices(2);
            await Assert.ThrowsAnyAsync<Autd3Exception>(async () =>
            {
                await using var client = await Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());
            });
        }

        [Fact]
        public void AMalformedGroupIsRejected()
        {
            var option = new TransportOption { Group = "127.0.0.1:1" };
            Assert.Throws<Autd3Exception>(() => option.CreateHandle());
        }

        [Fact]
        public void RebootRejectsAnOutOfRangeIndex()
        {
            using var emulator = new UdpEmulator(1);
            Assert.Throws<ArgumentOutOfRangeException>(() => emulator.Reboot(1));
        }
    }
}
