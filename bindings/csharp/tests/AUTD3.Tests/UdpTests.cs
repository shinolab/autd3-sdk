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
            Assert.Equal(TimeSpan.FromMilliseconds(200), option.ResponseTimeout);
            Assert.Equal(TimeSpan.FromSeconds(10), option.EnumerationTimeout);
            Assert.Equal(TimeSpan.FromSeconds(30), option.SyncTimeout);
            Assert.Null(option.SendRateLimit);
            Assert.Equal(8192ul, option.SendBuffer);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.TimerResolution);
        }

        [Fact]
        public void UnsetOptionalFieldsAreReadFromTheNativeLibrary()
        {
            var defaults = UdpOption.Defaults();
            var option = new UdpOption();
            Assert.Equal(defaults.SendRateLimit, option.SendRateLimit);
            Assert.Equal(90f, new UdpOption { SendRateLimit = 90f }.SendRateLimit);
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

        private static async Task<Autd3Exception> OpenFails(UdpOption option, ClientConfig config)
        {
            using var geometry = new Geometry(new[] { new Autd3(System.Numerics.Vector3.Zero) });
            return await Assert.ThrowsAsync<Autd3Exception>(() => Client.OpenAsync(geometry, option, config));
        }

        [Fact]
        public async Task AZeroAckTimeoutFailsTheOpenWithTheRustError()
        {
            var config = new ClientConfig { AckTimeout = TimeSpan.Zero };
            NativeClient.autd3_client_config_free(config.CreateHandle());
            var error = await OpenFails(new UdpOption(), config);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("ack_timeout", error.Message);
        }

        [Fact]
        public async Task AZeroExecLeadFailsTheOpenWithTheRustError()
        {
            var error = await OpenFails(new UdpOption(), new ClientConfig { ExecLead = TimeSpan.Zero });
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("exec_lead must be within", error.Message);
        }

        [Fact]
        public void ANegativeAckTimeoutIsRejected()
        {
            var config = new ClientConfig { AckTimeout = TimeSpan.FromTicks(-1) };
            var error = Assert.Throws<Autd3Exception>(() => config.CreateHandle());
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
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
            handle = zero.CreateHandle();
            try
            {
                Assert.Equal(TimeSpan.Zero, UdpOption.FromHandle(handle).Heartbeat);
            }
            finally
            {
                NativeClient.autd3_udp_option_free(handle);
            }
        }

        [Fact]
        public async Task AZeroHeartbeatFailsTheOpenWithTheRustError()
        {
            var error = await OpenFails(new UdpOption { Heartbeat = TimeSpan.Zero }, new ClientConfig());
            Assert.Equal(Autd3ErrorCode.Network, error.Code);
            Assert.Contains("heartbeat", error.Message);
        }

        [Fact]
        public void ATimerResolutionReachesTheNativeOption()
        {
            Assert.Equal(TimeSpan.FromMilliseconds(1), default(UdpOption).TimerResolution);
            foreach (var resolution in new TimeSpan?[] { null, TimeSpan.Zero, TimeSpan.FromMilliseconds(2) })
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

            var negative = new UdpOption { TimerResolution = TimeSpan.FromTicks(-1) };
            Assert.Throws<Autd3Exception>(() => negative.CreateHandle());
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

            handle = new UdpOption { SendRateLimit = 0 }.CreateHandle();
            try
            {
                Assert.Equal(0f, UdpOption.FromHandle(handle).SendRateLimit);
            }
            finally
            {
                NativeClient.autd3_udp_option_free(handle);
            }
        }

        [Fact]
        public async Task AZeroSendRateLimitFailsTheOpen()
        {
            var error = await OpenFails(new UdpOption { SendRateLimit = 0 }, new ClientConfig());
            Assert.Equal(Autd3ErrorCode.Network, error.Code);
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
            Assert.NotEqual(Interface.Auto, Interface.Simulator(44336));
            Assert.NotEqual(Interface.Name("eth0"), Interface.Simulator(44336));
            Assert.NotEqual(Interface.Simulator(44336), Interface.Simulator(50000));
            var handle = new UdpOption { Iface = Interface.Simulator(50000) }.CreateHandle();
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
        }

        [Fact]
        public async Task AZeroServerTimeoutFailsTheOpen()
        {
            NativeClient.autd3_server_link_free(new ServerLink { Timeout = TimeSpan.Zero }.CreateHandle());
            using var geometry = new Geometry(new[] { new Autd3(System.Numerics.Vector3.Zero) });
            var error = await Assert.ThrowsAsync<Autd3Exception>(() =>
                Client.OpenAsync(geometry, new ServerLink { Timeout = TimeSpan.Zero }, new ClientConfig()));
            Assert.Contains("must not be zero", error.Message);
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
