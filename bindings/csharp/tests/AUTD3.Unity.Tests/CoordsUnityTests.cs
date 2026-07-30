#if AUTD3_UNITY
using AUTD3;
using Xunit;

namespace AUTD3.Unity.Tests
{
    public class CoordsUnityTests
    {
        [Fact]
        public void PointScalesToMmAndFlipsZ()
        {
            var c = Coords.Point(new Vector3(0.1f, 0.2f, 0.3f));
            Assert.Equal(100.0, c.X, 3);
            Assert.Equal(200.0, c.Y, 3);
            Assert.Equal(-300.0, c.Z, 3);
        }

        [Fact]
        public void PointRoundTrips()
        {
            var v = new Vector3(0.1f, -0.2f, 0.3f);
            var back = Coords.FromPoint(Coords.Point(v));
            Assert.Equal(v.x, back.x, 4);
            Assert.Equal(v.y, back.y, 4);
            Assert.Equal(v.z, back.z, 4);
        }

        [Fact]
        public void DirFlipsZAndNormalizes()
        {
            var d = Coords.Dir(new Vector3(0f, 0f, 5f));
            Assert.Equal(0.0, d.X, 4);
            Assert.Equal(0.0, d.Y, 4);
            Assert.Equal(-1.0, d.Z, 4);
        }

        [Fact]
        public void DirRoundTrips()
        {
            var v = new Vector3(1f, 0f, 0f);
            var back = Coords.FromDir(Coords.Dir(v));
            Assert.Equal(v.x, back.x, 4);
            Assert.Equal(v.y, back.y, 4);
            Assert.Equal(v.z, back.z, 4);
        }

        [Fact]
        public void RotationMirrorsAxis()
        {
            // UnityEngine.Quaternion ctor is (x, y, z, w).
            var q = new Quaternion(0.1f, 0.2f, 0.3f, 0.4f);
            var c = Coords.Rotation(q);
            Assert.Equal(-0.1, c.X, 4);
            Assert.Equal(-0.2, c.Y, 4);
            Assert.Equal(0.3, c.Z, 4);
            Assert.Equal(0.4, c.W, 4);
        }

        [Fact]
        public void RotationRoundTrips()
        {
            var q = new Quaternion(0.1f, 0.2f, 0.3f, 0.4f);
            var back = Coords.FromRotation(Coords.Rotation(q));
            Assert.Equal(q.x, back.x, 4);
            Assert.Equal(q.y, back.y, 4);
            Assert.Equal(q.z, back.z, 4);
            Assert.Equal(q.w, back.w, 4);
        }

        [Fact]
        public void IdentityRotationIsIdentity()
        {
            var c = Coords.Rotation(Quaternion.identity);
            Assert.Equal(0.0, c.X, 5);
            Assert.Equal(0.0, c.Y, 5);
            Assert.Equal(0.0, c.Z, 5);
            Assert.Equal(1.0, c.W, 5);
        }
    }
}
#endif
