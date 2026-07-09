namespace AUTD3
{
    using SnVector3 = System.Numerics.Vector3;
    using SnQuaternion = System.Numerics.Quaternion;

    internal static class Coords
    {
#if UNITY_5_3_OR_NEWER || AUTD3_UNITY
        internal static SnVector3 Point(Vector3 v) => new SnVector3(v.x * 1000f, v.y * 1000f, -v.z * 1000f);

        internal static SnVector3 Dir(Vector3 v) => SnVector3.Normalize(new SnVector3(v.x, v.y, -v.z));

        internal static SnQuaternion Rotation(Quaternion q) => new SnQuaternion(-q.x, -q.y, q.z, q.w);

        internal static Vector3 FromPoint(SnVector3 v) => new Vector3(v.X * 0.001f, v.Y * 0.001f, -v.Z * 0.001f);

        internal static Vector3 FromDir(SnVector3 v)
        {
            var n = SnVector3.Normalize(v);
            return new Vector3(n.X, n.Y, -n.Z);
        }

        internal static Quaternion FromRotation(SnQuaternion q) => new Quaternion(-q.X, -q.Y, q.Z, q.W);

        internal static Quaternion IdentityRotation => Quaternion.identity;
#else
        internal static SnVector3 Point(Vector3 v) => v;

        internal static SnVector3 Dir(Vector3 v) => SnVector3.Normalize(v);

        internal static SnQuaternion Rotation(Quaternion q) => q;

        internal static Vector3 FromPoint(SnVector3 v) => v;

        internal static Vector3 FromDir(SnVector3 v) => SnVector3.Normalize(v);

        internal static Quaternion FromRotation(SnQuaternion q) => q;

        internal static Quaternion IdentityRotation => Quaternion.Identity;
#endif

        internal static float[] PointArray(Vector3 v)
        {
            var c = Point(v);
            return new[] { c.X, c.Y, c.Z };
        }

        internal static float[] DirArray(Vector3 v)
        {
            var c = Dir(v);
            return new[] { c.X, c.Y, c.Z };
        }

        internal static Vector3 FromPointArray(float[] xyz) => FromPoint(new SnVector3(xyz[0], xyz[1], xyz[2]));

        internal static Vector3 FromDirArray(float[] xyz) => FromDir(new SnVector3(xyz[0], xyz[1], xyz[2]));
    }
}
