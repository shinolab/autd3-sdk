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
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.AckTimeout);
            Assert.Equal(8192ul, option.SendBufferBytes);
        }

        [Fact]
        public void UnsetOptionalFieldsAreReadFromTheNativeLibrary()
        {
            var defaults = TransportOption.Defaults();
            var option = new TransportOption();
            Assert.Equal(defaults.SendBufferBytes, option.SendBufferBytes);
            Assert.Null(new TransportOption { SendBufferBytes = null }.SendBufferBytes);
        }

        [Fact]
        public void AnUnsetFieldKeepsTheNativeDefault()
        {
            var option = new TransportOption();
            Assert.Equal(Interface.Auto, option.Iface);
            Assert.Equal(TimeSpan.FromSeconds(40), option.OpenTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.AckTimeout);
        }

        [Fact]
        public void ANegativeAckTimeoutIsRejected()
        {
            var option = new TransportOption { AckTimeout = TimeSpan.FromTicks(-1) };
            var error = Assert.Throws<Autd3Exception>(() => option.CreateHandle());
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
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
