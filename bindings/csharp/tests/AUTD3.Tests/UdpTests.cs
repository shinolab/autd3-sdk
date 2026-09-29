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
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.Heartbeat);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.ReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
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
            Assert.Null(option.Heartbeat);
            Assert.Null(option.LostTimeout);
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
                Assert.True(checker.Check().AllReady);
                var telemetry = await client.ReadTelemetryAsync();
                Assert.Equal(2, telemetry.Count);
                foreach (var counters in telemetry)
                {
                    Assert.Equal(TelemetryCounters.Count, counters.AsArray().Count);
                    Assert.Equal(counters.Get(Telemetry.Failsafe), counters[Telemetry.Failsafe]);
                }
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
        public void AZeroAckTimeoutIsRejected()
        {
            var config = new ClientConfig { AckTimeout = TimeSpan.Zero };
            Assert.Throws<Autd3Exception>(() => config.CreateHandle());
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
