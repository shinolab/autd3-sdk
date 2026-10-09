using System;
using System.Linq;
using System.Threading.Tasks;
using Xunit;

namespace AUTD3.Tests
{
    public class UdpTests
    {
        [Fact]
        public void TheNativeDefaultsFollowTheSpec()
        {
            var option = TransportOption.Defaults();
            Assert.Equal(Interface.Auto, option.Iface);
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.Heartbeat);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.ReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(200), option.ResponseTimeout);
            Assert.Equal(TimeSpan.FromSeconds(10), option.EnumerationTimeout);
            Assert.Equal(TimeSpan.FromSeconds(30), option.SyncTimeout);
            Assert.Null(option.SendRateLimit);
            Assert.Equal(8192u, option.SendBuffer);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.TimerResolution);
        }

        [Fact]
        public void AnUnsetFieldKeepsTheNativeDefault()
        {
            var option = new TransportOption();
            Assert.Equal(Interface.Auto, option.Iface);
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.Heartbeat);
            Assert.Null(new TransportOption { Heartbeat = null }.Heartbeat);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.ReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(200), option.ResponseTimeout);
            Assert.Equal(TimeSpan.FromSeconds(10), option.EnumerationTimeout);
            Assert.Equal(TimeSpan.FromSeconds(30), option.SyncTimeout);
        }

        [Fact]
        public void TheDefaultValueKeepsTheHeartbeat()
        {
            Assert.Equal(TimeSpan.FromMilliseconds(10), default(TransportOption).Heartbeat);
        }

        [Fact]
        public async Task TheClientOpensTheEmulatedChain()
        {
            using var emulator = new UdpEmulator(2);
            var option = emulator.Option();
            Assert.NotNull(option.Iface.AddrValue);
            using var geometry = Fixture.Devices(2);
            var client = await Client.OpenAsync(geometry, option, new ClientConfig());
            using var checker = client.StateChecker();
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
            Assert.Throws<Autd3Exception>(() => checker.Check());
        }

        [Fact]
        public async Task ADeviceCountMismatchFailsToOpen()
        {
            using var emulator = new UdpEmulator(1);
            using var geometry = Fixture.Devices(2);
            await Assert.ThrowsAnyAsync<Autd3Exception>(() => Client.OpenAsync(geometry, emulator.Option(), new ClientConfig()));
        }

        [Fact]
        public void AZeroAckTimeoutIsRejected()
        {
            var config = new ClientConfig { AckTimeout = TimeSpan.Zero };
            Assert.Throws<Autd3Exception>(() => config.CreateHandle());
        }

        [Fact]
        public void ADisabledHeartbeatReachesTheNativeOption()
        {
            var handle = new TransportOption { Heartbeat = null }.CreateHandle();
            try
            {
                Assert.Null(TransportOption.FromHandle(handle).Heartbeat);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }

            var zero = new TransportOption { Heartbeat = TimeSpan.Zero };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void ATimerResolutionReachesTheNativeOption()
        {
            Assert.Equal(TimeSpan.FromMilliseconds(1), default(TransportOption).TimerResolution);
            foreach (var resolution in new TimeSpan?[] { null, TimeSpan.FromMilliseconds(2) })
            {
                var handle = new TransportOption { TimerResolution = resolution }.CreateHandle();
                try
                {
                    Assert.Equal(resolution, TransportOption.FromHandle(handle).TimerResolution);
                }
                finally
                {
                    NativeClient.autd3_transport_option_free(handle);
                }
            }

            var zero = new TransportOption { TimerResolution = TimeSpan.Zero };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void ASendBufferReachesTheNativeOption()
        {
            Assert.Equal(8192u, default(TransportOption).SendBuffer);
            foreach (var bytes in new uint?[] { null, 65536 })
            {
                var handle = new TransportOption { SendBuffer = bytes }.CreateHandle();
                try
                {
                    Assert.Equal(bytes, TransportOption.FromHandle(handle).SendBuffer);
                }
                finally
                {
                    NativeClient.autd3_transport_option_free(handle);
                }
            }

            var zero = new TransportOption { SendBuffer = 0 };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void ASendRateLimitReachesTheNativeOption()
        {
            Assert.Null(default(TransportOption).SendRateLimit);
            var handle = new TransportOption { SendRateLimit = 95 }.CreateHandle();
            try
            {
                Assert.Equal(95f, TransportOption.FromHandle(handle).SendRateLimit);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }

            var zero = new TransportOption { SendRateLimit = 0 };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void AMalformedAddressIsRejected()
        {
            var option = new TransportOption { Iface = Interface.Addr("127.0.0.1:1") };
            Assert.Throws<Autd3Exception>(() => option.CreateHandle());
        }

        [Fact]
        public void TheSimulatorInterfaceIsDistinctAndReachesTheNativeOption()
        {
            Assert.NotEqual(Interface.Auto, Interface.Simulator);
            Assert.NotEqual(Interface.Name("eth0"), Interface.Simulator);
            var handle = new TransportOption { Iface = Interface.Simulator }.CreateHandle();
            NativeClient.autd3_transport_option_free(handle);
        }

        [Fact]
        public void RebootRejectsAnOutOfRangeIndex()
        {
            using var emulator = new UdpEmulator(1);
            Assert.Throws<ArgumentOutOfRangeException>(() => emulator.Reboot(1));
        }
    }
}
