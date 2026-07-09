#if UNITY_5_3_OR_NEWER || AUTD3_UNITY
global using Vector3 = UnityEngine.Vector3;
global using Quaternion = UnityEngine.Quaternion;
#else
global using Vector3 = System.Numerics.Vector3;
global using Quaternion = System.Numerics.Quaternion;
#endif
