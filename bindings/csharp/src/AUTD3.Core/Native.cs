using System;
using System.Runtime.InteropServices;
using System.Text;

namespace AUTD3
{
    internal static class NativeCore
    {
        private const string Lib = "autd3_core";

        [StructLayout(LayoutKind.Sequential)]
        internal struct Autd3Device
        {
            public float Ox, Oy, Oz;
            public float Rw, Rx, Ry, Rz;
        }

        [DllImport(Lib)]
        internal static extern IntPtr autd3_core_geometry_new(Autd3Device[] devices, UIntPtr len);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_core_geometry_num_devices(IntPtr geometry);

        [DllImport(Lib)]
        internal static extern void autd3_core_geometry_center(IntPtr geometry, [Out] float[] outXyz);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_core_geometry_num_transducers(IntPtr geometry);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_core_device_num_transducers(IntPtr geometry, UIntPtr dev);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_core_device_idx(IntPtr geometry, UIntPtr dev);

        [DllImport(Lib)]
        internal static extern void autd3_core_device_rotation(IntPtr geometry, UIntPtr dev, [Out] float[] outWijk);

        [DllImport(Lib)]
        internal static extern void autd3_core_device_direction_x(IntPtr geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib)]
        internal static extern void autd3_core_device_direction_y(IntPtr geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib)]
        internal static extern void autd3_core_device_direction_axial(IntPtr geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib)]
        internal static extern void autd3_core_geometry_free(IntPtr geometry);

        [DllImport(Lib)]
        internal static extern float autd3_core_phase_radian(byte value);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_core_sampling_config_freq_4k();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_core_sampling_config_freq_40k();

        [DllImport(Lib)]
        internal static extern IntPtr autd3_core_sampling_config_divide(ushort divide);

        [DllImport(Lib)]
        internal static extern int autd3_core_sampling_config_divide_value(IntPtr config, out ushort outValue);

        [DllImport(Lib)]
        internal static extern void autd3_core_sampling_config_free(IntPtr config);
    }

    internal static class NativeUtil
    {
        internal static string Utf8(byte[] buffer)
        {
            var n = Array.IndexOf<byte>(buffer, 0);
            if (n < 0)
            {
                n = buffer.Length;
            }
            return Encoding.UTF8.GetString(buffer, 0, n);
        }

        internal static string PtrToString(IntPtr ptr)
        {
            return ptr == IntPtr.Zero ? string.Empty : Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
        }
    }
}
