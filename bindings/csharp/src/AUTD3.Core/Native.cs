using System;
using System.Runtime.InteropServices;
using System.Text;

namespace AUTD3
{
    internal static class NativeCore
    {
        private const string Lib = "autd3_core";

        static NativeCore() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [StructLayout(LayoutKind.Sequential)]
        internal struct Autd3Device
        {
            public float Ox, Oy, Oz;
            public float Rw, Rx, Ry, Rz;
        }

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_geometry_new(Autd3Device[] devices, UIntPtr len, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_geometry_from_json([MarshalAs(UnmanagedType.LPUTF8Str)] string json, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_geometry_to_json(GeometryHandle geometry, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_free_string(IntPtr ptr);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_geometry_num_devices(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_geometry_center(GeometryHandle geometry, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_geometry_num_transducers(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_device_num_transducers(GeometryHandle geometry, UIntPtr dev);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_device_idx(GeometryHandle geometry, UIntPtr dev);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_rotation(GeometryHandle geometry, UIntPtr dev, [Out] float[] outWijk);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_center(GeometryHandle geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_direction_x(GeometryHandle geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_direction_y(GeometryHandle geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_direction_axial(GeometryHandle geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_transducer_position(GeometryHandle geometry, UIntPtr dev, UIntPtr tr, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_transducer_direction(GeometryHandle geometry, UIntPtr dev, UIntPtr tr, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_geometry_clone(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_geometry_free(IntPtr geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_device_positions(GeometryHandle geometry, UIntPtr dev, [Out] float[] dst, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_device_to_local(GeometryHandle geometry, UIntPtr dev, float[] point, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte autd3_core_phase_from_rad(float radian);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong autd3_core_params_ultrasound_period_ns();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern uint autd3_core_params_ultrasound_freq_hz();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_params_mod_buffer_samples();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_params_buffer_size_min();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_params_emission_max_indices();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte autd3_core_params_num_foci_max();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ushort autd3_core_params_pulse_width_period();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_params_max_inflight();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_params_num_transducers();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern uint autd3_core_params_grid_x();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern uint autd3_core_params_grid_y();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_core_params_pitch_mm();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_core_params_device_width_mm();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_core_params_device_height_mm();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_divide(ushort divide);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_freq(float hz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_freq_nearest(float hz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_period(ulong nanos);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_period_nearest(ulong nanos);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_sampling_config_resolve(IntPtr config, out ushort @out, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_sampling_config_free(IntPtr config);
    }

    internal static class NativeAbi
    {
        internal const ushort Major = 0;
        internal const ushort Minor = 9;
        internal const ushort Patch = 0;

        internal const int ErrorBufferLength = 1024;

        internal static void Verify(string library, uint actual)
        {
            var expected = ((uint)Major << 20) | ((uint)Minor << 10) | Patch;
            if (actual == expected)
            {
                return;
            }
            throw new Autd3Exception(
                $"native library '{library}' reports C ABI version {actual >> 20}.{(actual >> 10) & 0x3FF}.{actual & 0x3FF}, " +
                $"but this binding requires exactly {Major}.{Minor}.{Patch}. " +
                "The managed package and the native library are from different releases.");
        }
    }

    internal static class OptionNative
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        internal delegate int SetDurationFn(IntPtr option, ulong ns);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        internal delegate int GetDurationFn(IntPtr option, out ulong ns);

        internal static void Apply(string field, int code)
        {
            if (code != 0)
            {
                throw new Autd3Exception($"`{field}` is out of the range the native library accepts", Autd3ErrorCode.InvalidArgument);
            }
        }

        internal static void SetRequiredDuration(IntPtr option, string field, TimeSpan value, SetDurationFn set)
        {
            Apply(field, value < TimeSpan.Zero ? -1 : set(option, ToNanos(value)));
        }

        internal static TimeSpan GetDuration(IntPtr option, GetDurationFn get)
        {
            Apply("preset", get(option, out var ns));
            return FromNanos(ns);
        }

        internal static ulong ToNanos(TimeSpan value) =>
            (ulong)value.Ticks > ulong.MaxValue / 100UL ? ulong.MaxValue : (ulong)value.Ticks * 100UL;

        internal static TimeSpan FromNanos(ulong ns) => TimeSpan.FromTicks((long)(ns / 100));
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
