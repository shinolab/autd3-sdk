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
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.ReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(200), option.ResponseTimeout);
            Assert.Equal(TimeSpan.FromSeconds(10), option.EnumerationTimeout);
            Assert.Equal(TimeSpan.FromSeconds(30), option.SyncTimeout);
            Assert.Null(option.SendRateLimit);
            Assert.Equal(8192ul, option.SendBuffer);
        }

        [Fact]
        public void UnsetOptionalFieldsAreReadFromTheNativeLibrary()
        {
            var defaults = TransportOption.Defaults();
            var option = new TransportOption();
            Assert.Equal(defaults.SendRateLimit, option.SendRateLimit);
            Assert.Equal(90f, new TransportOption { SendRateLimit = 90f }.SendRateLimit);
        }

        [Fact]
        public void AnUnsetFieldKeepsTheNativeDefault()
        {
            var option = new TransportOption();
            Assert.Equal(Interface.Auto, option.Iface);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.ReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(200), option.ResponseTimeout);
            Assert.Equal(TimeSpan.FromSeconds(10), option.EnumerationTimeout);
            Assert.Equal(TimeSpan.FromSeconds(30), option.SyncTimeout);
        }

        private static async Task<Autd3Exception> OpenFails(TransportOption option, ClientConfig config)
        {
            using var geometry = new Geometry(new[] { new Autd3(System.Numerics.Vector3.Zero) });
            return await Assert.ThrowsAsync<Autd3Exception>(() => Client.OpenAsync(geometry, option, config));
        }

        [Fact]
        public async Task AZeroAckTimeoutFailsTheOpenWithTheRustError()
        {
            var config = new ClientConfig { AckTimeout = TimeSpan.Zero };
            NativeClient.autd3_client_config_free(config.CreateHandle());
            var error = await OpenFails(new TransportOption(), config);
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("ack_timeout", error.Message);
        }

        [Fact]
        public void ANegativeAckTimeoutIsRejected()
        {
            var config = new ClientConfig { AckTimeout = TimeSpan.FromTicks(-1) };
            var error = Assert.Throws<Autd3Exception>(() => config.CreateHandle());
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
        }

        [Fact]
        public async Task AZeroLostTimeoutFailsTheOpenWithTheRustError()
        {
            var error = await OpenFails(new TransportOption { LostTimeout = TimeSpan.Zero }, new ClientConfig());
            Assert.Equal(Autd3ErrorCode.Network, error.Code);
            Assert.Contains("lost_timeout", error.Message);
        }

        [Fact]
        public void ASendBufferReachesTheNativeOption()
        {
            Assert.Equal(8192ul, default(TransportOption).SendBuffer);
            foreach (var bytes in new ulong?[] { null, 65536, 1ul << 32 })
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

            handle = new TransportOption { SendRateLimit = 0 }.CreateHandle();
            try
            {
                Assert.Equal(0f, TransportOption.FromHandle(handle).SendRateLimit);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }
        }

        [Fact]
        public async Task AZeroSendRateLimitFailsTheOpen()
        {
            var error = await OpenFails(new TransportOption { SendRateLimit = 0 }, new ClientConfig());
            Assert.Equal(Autd3ErrorCode.Network, error.Code);
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
            Assert.NotEqual(Interface.Auto, Interface.Simulator(44336));
            Assert.NotEqual(Interface.Name("eth0"), Interface.Simulator(44336));
            Assert.NotEqual(Interface.Simulator(44336), Interface.Simulator(50000));
            var handle = new TransportOption { Iface = Interface.Simulator(50000) }.CreateHandle();
            NativeClient.autd3_transport_option_free(handle);
        }
    }
}
