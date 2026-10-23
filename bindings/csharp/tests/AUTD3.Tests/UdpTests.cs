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
            var option = new TransportOption();
            Assert.Equal(Interface.Auto, option.Iface);
            Assert.Equal(TimeSpan.FromSeconds(40), option.OpenTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(100), option.LostTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(10), option.AckTimeout);
            Assert.Equal(8192ul, option.SendBufferBytes);
            Assert.True(option.SyncOnOpen);
            Assert.True(default(TransportOption).SyncOnOpen);
        }

        [Fact]
        public void UnsetOptionalFieldsAreReadFromTheNativeLibrary()
        {
            Assert.Equal(TransportOptionDefaults.SendBufferBytes, new TransportOption().SendBufferBytes);
            Assert.Null(new TransportOption { SendBufferBytes = null }.SendBufferBytes);
        }

        [Fact]
        public void EveryFieldReachesTheNativeOption()
        {
            var native = new TransportOption
            {
                OpenTimeout = TimeSpan.FromSeconds(1),
                LostTimeout = TimeSpan.FromMilliseconds(2),
                AckTimeout = TimeSpan.FromMilliseconds(25),
                SyncOnOpen = false,
            }.ToNative();
            Assert.Equal(1_000_000_000ul, native.OpenTimeoutNs);
            Assert.Equal(2_000_000ul, native.LostTimeoutNs);
            Assert.Equal(25_000_000ul, native.AckTimeoutNs);
            Assert.Equal((byte)0, native.SyncOnOpen);
            Assert.Equal((byte)1, new TransportOption().ToNative().SyncOnOpen);
        }

        [Fact]
        public void ANegativeAckTimeoutIsRejected()
        {
            var option = new TransportOption { AckTimeout = TimeSpan.FromTicks(-1) };
            var error = Assert.Throws<Autd3Exception>(() => option.ToNative());
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("ackTimeout", error.Message);
        }

        [Fact]
        public void ASendBufferReachesTheNativeOption()
        {
            Assert.Equal(8192ul, default(TransportOption).SendBufferBytes);
            foreach (var bytes in new ulong?[] { null, 65536, 1ul << 32 })
            {
                Assert.Equal(bytes ?? 0, new TransportOption { SendBufferBytes = bytes }.ToNative().SendBufferBytes);
            }

            var zero = new TransportOption { SendBufferBytes = 0 };
            Assert.Throws<Autd3Exception>(() => zero.ToNative());
        }

        [Fact]
        public void AMalformedAddressIsRejectedBeforeTheOpen()
        {
            using var geometry = Fixture.SingleDevice();
            var option = new TransportOption { Iface = Interface.Addr("127.0.0.1:1") };
            var error = Assert.Throws<Autd3Exception>(() => Client.Open(geometry, option));
            Assert.Equal(Autd3ErrorCode.InvalidArgument, error.Code);
            Assert.Contains("127.0.0.1:1", error.Message);
        }

        [Fact]
        public void TheInterfaceKindsReachTheNativeOpen()
        {
            Assert.NotEqual(Interface.Auto, Interface.Addr("[::1]:44336"));
            Assert.NotEqual(Interface.Name("[::1]:44336"), Interface.Addr("[::1]:44336"));
            Assert.NotEqual(Interface.Addr("[::1]:44336"), Interface.Addr("[::1]:50000"));

            var auto = new TransportOption();
            Assert.Equal((byte)0, auto.IfaceKind);
            Assert.Null(auto.IfaceValue);
            var name = new TransportOption { Iface = Interface.Name("eth0") };
            Assert.Equal((byte)1, name.IfaceKind);
            Assert.Equal("eth0", name.IfaceValue);
            var addr = new TransportOption { Iface = Interface.Addr("[::1]:50000") };
            Assert.Equal((byte)2, addr.IfaceKind);
            Assert.Equal("[::1]:50000", addr.IfaceValue);
        }
    }
}
