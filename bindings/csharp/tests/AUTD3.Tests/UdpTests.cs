using System;
using System.Linq;
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
            Assert.Equal(TimeSpan.FromSeconds(40), option.OpenTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.RecoveryReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.AckTimeout);
            Assert.Null(option.SendRateLimitPercent);
            Assert.Equal(8192ul, option.SendBufferBytes);
        }

        [Fact]
        public void UnsetOptionalFieldsAreReadFromTheNativeLibrary()
        {
            var defaults = TransportOption.Defaults();
            var option = new TransportOption();
            Assert.Equal(defaults.SendRateLimitPercent, option.SendRateLimitPercent);
            Assert.Equal(90f, new TransportOption { SendRateLimitPercent = 90f }.SendRateLimitPercent);
        }

        [Fact]
        public void AnUnsetFieldKeepsTheNativeDefault()
        {
            var option = new TransportOption();
            Assert.Equal(Interface.Auto, option.Iface);
            Assert.Equal(TimeSpan.FromSeconds(40), option.OpenTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(1), option.RecoveryReplyTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.AckTimeout);
        }

        private static Autd3Exception OpenFails(TransportOption option)
        {
            using var geometry = new Geometry(new[] { new Autd3(System.Numerics.Vector3.Zero) });
            return Assert.Throws<Autd3Exception>(() => Client.Open(geometry, option));
        }

        [Fact]
        public void AZeroAckTimeoutFailsTheOpenWithTheRustError()
        {
            var option = new TransportOption { AckTimeout = TimeSpan.Zero };
            NativeClient.autd3_transport_option_free(option.CreateHandle());
            var error = OpenFails(option);
            Assert.Equal(Autd3ErrorCode.Network, error.Code);
            Assert.Contains("ack_timeout", error.Message);
        }

        [Fact]
        public void ANegativeAckTimeoutIsRejected()
        {
            var option = new TransportOption { AckTimeout = TimeSpan.FromTicks(-1) };
            var error = Assert.Throws<Autd3Exception>(() => option.CreateHandle());
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
        }

        [Fact]
        public void AZeroOpenTimeoutFailsTheOpenWithTheRustError()
        {
            var error = OpenFails(new TransportOption { OpenTimeout = TimeSpan.Zero });
            Assert.Equal(Autd3ErrorCode.Network, error.Code);
            Assert.Contains("open_timeout", error.Message);
        }

        [Fact]
        public void AZeroLostTimeoutFailsTheOpenWithTheRustError()
        {
            var error = OpenFails(new TransportOption { LostTimeout = TimeSpan.Zero });
            Assert.Equal(Autd3ErrorCode.Network, error.Code);
            Assert.Contains("lost_timeout", error.Message);
        }

        [Fact]
        public void ASendBufferReachesTheNativeOption()
        {
            Assert.Equal(8192ul, default(TransportOption).SendBufferBytes);
            foreach (var bytes in new ulong?[] { null, 65536, 1ul << 32 })
            {
                var handle = new TransportOption { SendBufferBytes = bytes }.CreateHandle();
                try
                {
                    Assert.Equal(bytes, TransportOption.FromHandle(handle).SendBufferBytes);
                }
                finally
                {
                    NativeClient.autd3_transport_option_free(handle);
                }
            }

            var zero = new TransportOption { SendBufferBytes = 0 };
            Assert.Throws<Autd3Exception>(() => zero.CreateHandle());
        }

        [Fact]
        public void ASendRateLimitReachesTheNativeOption()
        {
            Assert.Null(default(TransportOption).SendRateLimitPercent);
            var handle = new TransportOption { SendRateLimitPercent = 95 }.CreateHandle();
            try
            {
                Assert.Equal(95f, TransportOption.FromHandle(handle).SendRateLimitPercent);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }

            handle = new TransportOption { SendRateLimitPercent = 0 }.CreateHandle();
            try
            {
                Assert.Equal(0f, TransportOption.FromHandle(handle).SendRateLimitPercent);
            }
            finally
            {
                NativeClient.autd3_transport_option_free(handle);
            }
        }

        [Fact]
        public void AZeroSendRateLimitFailsTheOpen()
        {
            var error = OpenFails(new TransportOption { SendRateLimitPercent = 0 });
            Assert.Equal(Autd3ErrorCode.Network, error.Code);
        }

        [Fact]
        public void AMalformedAddressIsRejected()
        {
            var option = new TransportOption { Iface = Interface.Addr("127.0.0.1:1") };
            Assert.Throws<Autd3Exception>(() => option.CreateHandle());
        }

        [Fact]
        public void TheAddrInterfaceIsDistinctAndReachesTheNativeOption()
        {
            Assert.NotEqual(Interface.Auto, Interface.Addr("[::1]:44336"));
            Assert.NotEqual(Interface.Name("[::1]:44336"), Interface.Addr("[::1]:44336"));
            Assert.NotEqual(Interface.Addr("[::1]:44336"), Interface.Addr("[::1]:50000"));
            var handle = new TransportOption { Iface = Interface.Addr("[::1]:50000") }.CreateHandle();
            NativeClient.autd3_transport_option_free(handle);
        }
    }
}
