#if AUTD3_UNITY
using AUTD3;
using Xunit;
using SnVector3 = System.Numerics.Vector3;
using SnQuaternion = System.Numerics.Quaternion;

namespace AUTD3.Unity.Tests
{
    public class CoordsEquivarianceTests
    {
        private static Quaternion Unit(float x, float y, float z, float w)
        {
            var n = (float)System.Math.Sqrt(x * x + y * y + z * z + w * w);
            return new Quaternion(x / n, y / n, z / n, w / n);
        }

        private static readonly Quaternion[] Rotations =
        {
            Quaternion.identity,
            Unit(0f, 0.70710678f, 0f, 0.70710678f),
            Unit(0f, -0.70710678f, 0f, 0.70710678f),
            Unit(0.70710678f, 0f, 0f, 0.70710678f),
            Unit(0f, 0f, 0.70710678f, 0.70710678f),
            Unit(0.1f, 0.2f, 0.3f, 0.4f),
            Unit(-0.5f, 0.25f, 0.8f, -0.1f),
            Unit(0.3f, -0.6f, 0.1f, 0.9f),
        };

        private static void Close(float expected, float actual)
        {
            var tol = 1e-4f * System.Math.Max(1f, System.Math.Abs(expected));
            Assert.True(System.Math.Abs(expected - actual) <= tol, $"expected {expected}, actual {actual}");
        }

        private static void CloseVec(SnVector3 expected, SnVector3 actual)
        {
            var scale = System.Math.Max(expected.Length(), actual.Length());
            var tol = System.Math.Max(1e-3f, 1e-5f * scale);
            Assert.True(SnVector3.Distance(expected, actual) <= tol, $"expected {expected}, actual {actual}");
        }

        private static readonly Vector3[] Points =
        {
            new Vector3(1f, 0f, 0f),
            new Vector3(0f, 1f, 0f),
            new Vector3(0f, 0f, 1f),
            new Vector3(0.1f, -0.2f, 0.3f),
            new Vector3(-0.05f, 0.4f, -0.15f),
        };

        [Fact]
        public void PointIsEquivariantUnderRotation()
        {
            foreach (var q in Rotations)
            {
                foreach (var v in Points)
                {
                    var lhs = Coords.Point(q * v);
                    var rhs = SnVector3.Transform(Coords.Point(v), Coords.Rotation(q));
                    CloseVec(lhs, rhs);
                }
            }
        }

        [Fact]
        public void DirIsEquivariantUnderRotation()
        {
            foreach (var q in Rotations)
            {
                foreach (var v in Points)
                {
                    var lhs = Coords.Dir(q * v);
                    var rhs = SnVector3.Transform(Coords.Dir(v), Coords.Rotation(q));
                    Assert.Equal(lhs.X, rhs.X, 4);
                    Assert.Equal(lhs.Y, rhs.Y, 4);
                    Assert.Equal(lhs.Z, rhs.Z, 4);
                }
            }
        }

        [Fact]
        public void FromPointIsEquivariantUnderRotation()
        {
            foreach (var q in Rotations)
            {
                foreach (var v in Points)
                {
                    var c = Coords.Point(v);
                    var lhs = Coords.FromPoint(SnVector3.Transform(c, Coords.Rotation(q)));
                    var rhs = q * Coords.FromPoint(c);
                    Assert.Equal(lhs.x, rhs.x, 4);
                    Assert.Equal(lhs.y, rhs.y, 4);
                    Assert.Equal(lhs.z, rhs.z, 4);
                }
            }
        }

        [Fact]
        public void RotationIsAHomomorphism()
        {
            foreach (var a in Rotations)
            {
                foreach (var b in Rotations)
                {
                    var lhs = Coords.Rotation(a * b);
                    var rhs = Coords.Rotation(a) * Coords.Rotation(b);
                    if (lhs.X * rhs.X + lhs.Y * rhs.Y + lhs.Z * rhs.Z + lhs.W * rhs.W < 0f)
                    {
                        rhs = new SnQuaternion(-rhs.X, -rhs.Y, -rhs.Z, -rhs.W);
                    }
                    Assert.Equal(lhs.X, rhs.X, 4);
                    Assert.Equal(lhs.Y, rhs.Y, 4);
                    Assert.Equal(lhs.Z, rhs.Z, 4);
                    Assert.Equal(lhs.W, rhs.W, 4);
                }
            }
        }

        [Fact]
        public void PointPreservesDistanceUpToScale()
        {
            foreach (var a in Points)
            {
                foreach (var b in Points)
                {
                    var expected = Vector3.Distance(a, b) * 1000f;
                    var actual = SnVector3.Distance(Coords.Point(a), Coords.Point(b));
                    Close(expected, actual);
                }
            }
        }

        [Fact]
        public void DirPreservesDotProduct()
        {
            foreach (var a in Points)
            {
                foreach (var b in Points)
                {
                    var expected = Vector3.Dot(a.normalized, b.normalized);
                    var actual = SnVector3.Dot(Coords.Dir(a), Coords.Dir(b));
                    Assert.Equal(expected, actual, 4);
                }
            }
        }

        [Fact]
        public void HandednessIsFlipped()
        {
            foreach (var a in Points)
            {
                foreach (var b in Points)
                {
                    var lhs = Coords.Point(Vector3.Cross(a, b));
                    var rhs = SnVector3.Cross(Coords.Point(a), Coords.Point(b));
                    CloseVec(lhs * 1000f, -rhs);
                }
            }
        }

        [Fact]
        public void DeviceAxesUnderIdentityRotation()
        {
            var x = Coords.FromDir(new SnVector3(1f, 0f, 0f));
            var y = Coords.FromDir(new SnVector3(0f, 1f, 0f));
            var z = Coords.FromDir(new SnVector3(0f, 0f, 1f));
            Assert.Equal(1.0, x.x, 4);
            Assert.Equal(0.0, x.y, 4);
            Assert.Equal(0.0, x.z, 4);
            Assert.Equal(0.0, y.x, 4);
            Assert.Equal(1.0, y.y, 4);
            Assert.Equal(0.0, y.z, 4);
            Assert.Equal(0.0, z.x, 4);
            Assert.Equal(0.0, z.y, 4);
            Assert.Equal(-1.0, z.z, 4);
        }
    }
}
#endif
