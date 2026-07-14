using System;
using System.Collections;
using System.Collections.Generic;
using System.Numerics;

namespace AUTD3
{
    public sealed class Geometry : IDisposable, IEnumerable<Device>
    {
        internal IntPtr Handle { get; private set; }

        public Geometry(IReadOnlyList<Autd3> devices)
        {
            var native = new NativeCore.Autd3Device[devices.Count];
            for (var i = 0; i < devices.Count; i++)
            {
                native[i] = devices[i].ToNative();
            }
            var handle = NativeCore.autd3_core_geometry_new(native, (UIntPtr)native.Length);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create geometry");
            }
            Handle = handle;
        }

        public int NumDevices => (int)NativeCore.autd3_core_geometry_num_devices(Handle);

        public int NumTransducers => (int)NativeCore.autd3_core_geometry_num_transducers(Handle);

        public bool IsEmpty => NumDevices == 0;

        public Vector3 Center
        {
            get
            {
                var xyz = new float[3];
                NativeCore.autd3_core_geometry_center(Handle, xyz);
                return Coords.FromPointArray(xyz);
            }
        }

        public Device this[int dev] => new Device(Handle, (UIntPtr)dev);

        public IEnumerator<Device> GetEnumerator()
        {
            var count = NumDevices;
            for (var i = 0; i < count; i++)
            {
                yield return this[i];
            }
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        public void Dispose()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeCore.autd3_core_geometry_free(Handle);
                Handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~Geometry()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeCore.autd3_core_geometry_free(Handle);
            }
        }
    }

    public readonly struct Device
    {
        private readonly IntPtr _geometry;
        private readonly UIntPtr _dev;

        internal Device(IntPtr geometry, UIntPtr dev)
        {
            _geometry = geometry;
            _dev = dev;
        }

        internal IntPtr GeometryHandle => _geometry;

        internal UIntPtr DeviceIndex => _dev;

        public int Idx => (int)NativeCore.autd3_core_device_idx(_geometry, _dev);

        public int NumTransducers => (int)NativeCore.autd3_core_device_num_transducers(_geometry, _dev);

        public bool IsEmpty => NumTransducers == 0;

        public Quaternion Rotation
        {
            get
            {
                var wijk = new float[4];
                NativeCore.autd3_core_device_rotation(_geometry, _dev, wijk);
                return Coords.FromRotation(new System.Numerics.Quaternion(wijk[1], wijk[2], wijk[3], wijk[0]));
            }
        }

        public Vector3 Center
        {
            get
            {
                var xyz = new float[3];
                NativeCore.autd3_core_device_center(_geometry, _dev, xyz);
                return Coords.FromPointArray(xyz);
            }
        }

        public Vector3 XDirection => DeviceDirection(NativeCore.autd3_core_device_direction_x);

        public Vector3 YDirection => DeviceDirection(NativeCore.autd3_core_device_direction_y);

        public Vector3 AxialDirection => DeviceDirection(NativeCore.autd3_core_device_direction_axial);

        public Vector3 Position(int tr)
        {
            var xyz = new float[3];
            if (NativeCore.autd3_core_transducer_position(_geometry, _dev, (UIntPtr)tr, xyz) != 0)
                throw new ArgumentOutOfRangeException(nameof(tr));
            return Coords.FromPointArray(xyz);
        }

        public Vector3 Direction(int tr)
        {
            var xyz = new float[3];
            if (NativeCore.autd3_core_transducer_direction(_geometry, _dev, (UIntPtr)tr, xyz) != 0)
                throw new ArgumentOutOfRangeException(nameof(tr));
            return Coords.FromDirArray(xyz);
        }

        public Vector3[] Positions
        {
            get
            {
                var n = NumTransducers;
                var result = new Vector3[n];
                for (var i = 0; i < n; i++)
                    result[i] = Position(i);
                return result;
            }
        }

        public Vector3[] Directions
        {
            get
            {
                var n = NumTransducers;
                var result = new Vector3[n];
                for (var i = 0; i < n; i++)
                    result[i] = Direction(i);
                return result;
            }
        }

        private Vector3 DeviceDirection(Action<IntPtr, UIntPtr, float[]> native)
        {
            var xyz = new float[3];
            native(_geometry, _dev, xyz);
            return Coords.FromDirArray(xyz);
        }
    }
}
