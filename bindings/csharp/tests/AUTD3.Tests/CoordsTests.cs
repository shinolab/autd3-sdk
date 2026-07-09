using System.Numerics;
using AUTD3;
using Xunit;

namespace AUTD3.Tests
{
    public class CoordsTests
    {
        [Fact]
        public void PointIsIdentityOnDotnet()
        {
            var v = new Vector3(1f, 2f, 3f);
            var c = Coords.Point(v);
            Assert.Equal(1f, c.X);
            Assert.Equal(2f, c.Y);
            Assert.Equal(3f, c.Z);
            var back = Coords.FromPoint(c);
            Assert.Equal(v, back);
        }

        [Fact]
        public void RotationIsIdentityOnDotnet()
        {
            var q = Quaternion.CreateFromAxisAngle(new Vector3(0f, 1f, 0f), 0.5f);
            var c = Coords.Rotation(q);
            Assert.Equal(q, c);
            Assert.Equal(q, Coords.FromRotation(c));
        }

        [Fact]
        public void DirNormalizesOnDotnet()
        {
            var d = Coords.Dir(new Vector3(0f, 0f, 5f));
            Assert.Equal(0f, d.X, 5);
            Assert.Equal(0f, d.Y, 5);
            Assert.Equal(1f, d.Z, 5);
        }

        [Fact]
        public void FromDirNormalizes()
        {
            var d = Coords.FromDir(new Vector3(3f, 0f, 0f));
            Assert.Equal(1f, d.X, 5);
            Assert.Equal(0f, d.Y, 5);
            Assert.Equal(0f, d.Z, 5);
        }

        [Fact]
        public void PointArrayReturnsComponents()
        {
            var a = Coords.PointArray(new Vector3(4f, 5f, 6f));
            Assert.Equal(new[] { 4f, 5f, 6f }, a);
        }

        [Fact]
        public void DirArrayIsNormalized()
        {
            var a = Coords.DirArray(new Vector3(0f, 2f, 0f));
            Assert.Equal(0f, a[0], 5);
            Assert.Equal(1f, a[1], 5);
            Assert.Equal(0f, a[2], 5);
        }
    }
}
