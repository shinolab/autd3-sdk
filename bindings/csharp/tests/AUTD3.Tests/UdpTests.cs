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
            var option = UdpOption.Defaults();
            Assert.Equal(Interface.Auto, option.Iface);
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.Heartbeat);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.ReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Null(option.RecoveryInterval);
            Assert.Equal(TimeSpan.FromMilliseconds(200), option.ResponseTimeout);
            Assert.Equal(TimeSpan.FromSeconds(10), option.EnumerationTimeout);
            Assert.Equal(TimeSpan.FromSeconds(30), option.SyncTimeout);
            Assert.Null(option.SendRateLimit);
            Assert.Equal(8192ul, option.SendBuffer);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.TimerResolution);
        }

        [Fact]
        public void AnUnsetFieldKeepsTheNativeDefault()
        {
            var option = new UdpOption();
            Assert.Equal(Interface.Auto, option.Iface);
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.Heartbeat);
            Assert.Null(new UdpOption { Heartbeat = null }.Heartbeat);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.ReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(200), option.ResponseTimeout);
            Assert.Equal(TimeSpan.FromSeconds(10), option.EnumerationTimeout);
            Assert.Equal(TimeSpan.FromSeconds(30), option.SyncTimeout);
        }

        [Fact]
        public void TheDefaultValueKeepsTheHeartbeat()
        {
            Assert.Equal(TimeSpan.FromMilliseconds(10), default(UdpOption).Heartbeat);
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
            var handle = new UdpOption { Heartbeat = null }.CreateHandle();
            try
            {
                Assert.Null(UdpOption.FromHandle(handle).Heartbeat);
            }
            finally
            {
                NativeClient.autd3_udp_option_free(handle);
            }

            var zero = new UdpOption { Heartbeat = TimeSpan.Zero };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void ARecoveryIntervalReachesTheNativeOption()
        {
            Assert.Null(default(UdpOption).RecoveryInterval);
            foreach (var interval in new TimeSpan?[] { null, TimeSpan.FromMilliseconds(100) })
            {
                var handle = new UdpOption { RecoveryInterval = interval }.CreateHandle();
                try
                {
                    Assert.Equal(interval, UdpOption.FromHandle(handle).RecoveryInterval);
                }
                finally
                {
                    NativeClient.autd3_udp_option_free(handle);
                }
            }

            var zero = new UdpOption { RecoveryInterval = TimeSpan.Zero };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void ATimerResolutionReachesTheNativeOption()
        {
            Assert.Equal(TimeSpan.FromMilliseconds(1), default(UdpOption).TimerResolution);
            foreach (var resolution in new TimeSpan?[] { null, TimeSpan.FromMilliseconds(2) })
            {
                var handle = new UdpOption { TimerResolution = resolution }.CreateHandle();
                try
                {
                    Assert.Equal(resolution, UdpOption.FromHandle(handle).TimerResolution);
                }
                finally
                {
                    NativeClient.autd3_udp_option_free(handle);
                }
            }

            var zero = new UdpOption { TimerResolution = TimeSpan.Zero };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void ASendBufferReachesTheNativeOption()
        {
            Assert.Equal(8192ul, default(UdpOption).SendBuffer);
            foreach (var bytes in new ulong?[] { null, 65536, 1ul << 32 })
            {
                var handle = new UdpOption { SendBuffer = bytes }.CreateHandle();
                try
                {
                    Assert.Equal(bytes, UdpOption.FromHandle(handle).SendBuffer);
                }
                finally
                {
                    NativeClient.autd3_udp_option_free(handle);
                }
            }

            var zero = new UdpOption { SendBuffer = 0 };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void ASendRateLimitReachesTheNativeOption()
        {
            Assert.Null(default(UdpOption).SendRateLimit);
            var handle = new UdpOption { SendRateLimit = 95 }.CreateHandle();
            try
            {
                Assert.Equal(95f, UdpOption.FromHandle(handle).SendRateLimit);
            }
            finally
            {
                NativeClient.autd3_udp_option_free(handle);
            }

            var zero = new UdpOption { SendRateLimit = 0 };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void AMalformedAddressIsRejected()
        {
            var option = new UdpOption { Iface = Interface.Addr("127.0.0.1:1") };
            Assert.Throws<Autd3Exception>(() => option.CreateHandle());
        }

        [Fact]
        public void TheSimulatorInterfaceIsDistinctAndReachesTheNativeOption()
        {
            Assert.NotEqual(Interface.Auto, Interface.Simulator);
            Assert.NotEqual(Interface.Name("eth0"), Interface.Simulator);
            var handle = new UdpOption { Iface = Interface.Simulator }.CreateHandle();
            NativeClient.autd3_udp_option_free(handle);
        }

        [Fact]
        public void AServerLinkDefaultsToTheWellKnownPortAndReachesTheNativeLink()
        {
            Assert.Equal((ushort)44338, new ServerLink().Port);
            Assert.Equal(TimeSpan.FromSeconds(1), default(ServerLink).Timeout);
            Assert.Equal(new ServerLink(), default(ServerLink));
            var link = new ServerLink { Port = 5000, Timeout = TimeSpan.FromMilliseconds(200) };
            var handle = link.CreateHandle();
            try
            {
                Assert.Equal(0, NativeClient.autd3_server_link_get_port(handle, out var port));
                Assert.Equal(0, NativeClient.autd3_server_link_get_timeout_ns(handle, out var ns));
                Assert.Equal((ushort)5000, port);
                Assert.Equal(200_000_000UL, ns);
            }
            finally
            {
                NativeClient.autd3_server_link_free(handle);
            }
            Assert.Throws<Autd3Exception>(() => new ServerLink { Timeout = TimeSpan.Zero }.CreateHandle());
        }

        [Fact]
        public async Task AClientWithoutAServerFailsAtOnce()
        {
            var probe = new System.Net.Sockets.TcpListener(System.Net.IPAddress.IPv6Loopback, 0);
            probe.Start();
            var port = (ushort)((System.Net.IPEndPoint)probe.LocalEndpoint).Port;
            probe.Stop();
            using var geometry = new Geometry(new[] { new Autd3(System.Numerics.Vector3.Zero) });
            var error = await Assert.ThrowsAsync<Autd3Exception>(() =>
                Client.OpenAsync(geometry, new ServerLink { Port = port }, new ClientConfig()));
            Assert.Contains("no server accepts", error.Message);
        }
    }
}
