using System;
using System.Collections.Generic;
using System.Numerics;

namespace AUTD3
{
    public sealed class Geometry : IDisposable
    {
        internal IntPtr Handle { get; private set; }

        public Geometry(IReadOnlyList<Device> devices)
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

        public Vector3 Center
        {
            get
            {
                var xyz = new float[3];
                NativeCore.autd3_core_geometry_center(Handle, xyz);
                return new Vector3(xyz[0], xyz[1], xyz[2]);
            }
        }

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
}
