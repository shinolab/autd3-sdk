#if AUTD3_UNITY
using AUTD3;
using Xunit;

namespace AUTD3.Unity.Tests
{
    public class DeviceSizeUnityTests
    {
        [Fact]
        public void DeviceSizeIsInMeters()
        {
            Assert.Equal(0.192, Autd3.DeviceWidth, 5);
            Assert.Equal(0.1514, Autd3.DeviceHeight, 5);
        }

        [Fact]
        public void SecondDeviceIsPlacedNextToFirst()
        {
            var second = new Autd3(new Vector3(Autd3.DeviceWidth, 0f, 0f));
            var native = second.ToNative();
            Assert.Equal(192.0, native.Ox, 3);
            Assert.Equal(0.0, native.Oy, 3);
            Assert.Equal(0.0, native.Oz, 3);

            using var geometry = new Geometry(new[] { new Autd3(Vector3.zero), second });
            var p0 = geometry[0].Position(0);
            var p1 = geometry[1].Position(0);
            Assert.Equal(0.192, p1.x - p0.x, 5);
            Assert.Equal(p0.y, p1.y, 5);
            Assert.Equal(p0.z, p1.z, 5);
        }
    }
}
#endif
