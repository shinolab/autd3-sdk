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
            Assert.Equal(8192ul, option.SendBuffer);
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
    }
}
