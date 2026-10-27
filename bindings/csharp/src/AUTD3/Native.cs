using System;
using System.Runtime.InteropServices;

namespace AUTD3
{


    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void CompletionCallback(int code, IntPtr value, IntPtr msg, IntPtr userData);

    internal static class NativeClient
    {
        private const string Lib = "autd3capi";

        static NativeClient() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_default(out TransportOptionNative option);






        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_params_pwe_table_size();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_silencer_default_completion_time(out ulong intensityNs, out ulong phaseNs, [MarshalAs(UnmanagedType.I1)] out bool strictMode);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_fpga_state_is_thermal_asserted(byte raw);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte autd3_fpga_state_current_mod_bank(byte raw);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern byte autd3_fpga_state_current_pattern_bank(byte raw);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_fpga_state_is_pattern_mode(byte raw);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_fpga_state_is_pattern_stopped(byte raw);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_fpga_state_is_mod_stopped(byte raw);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_fpga_state_is_transition_pending(byte raw);


        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_command_each(IntPtr[] ops, UIntPtr numDevices);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_op_free(IntPtr op);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_frames_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_frames_encode_into(FramesHandle frames, GeometryHandle geometry, IntPtr command, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_frames_num_frames(FramesHandle frames);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_frames_free(IntPtr frames);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_open(GeometryHandle geometry, byte ifaceKind, [MarshalAs(UnmanagedType.LPUTF8Str)] string? iface, TransportOptionNative option, out IntPtr client, byte[] outErr, UIntPtr outErrLen);

        [StructLayout(LayoutKind.Sequential)]
        internal struct DeviceStatusNative
        {
            public byte State;
            public byte PtpLocked;
            public byte FailsafeActive;
        }

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_device_status(ClientHandle client, [Out] DeviceStatusNative[] status, UIntPtr len, byte[] outErr, UIntPtr outErrLen);








        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_client_num_devices(ClientHandle client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_device_time_now(ClientHandle client, out ulong ns, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_send_heartbeat(ClientHandle client, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send_checked(ClientHandle client, FramesHandle frames, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send(ClientHandle client, FramesHandle frames, UIntPtr frame, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send_at(ClientHandle client, FramesHandle frames, UIntPtr frame, ExecAtNative exec, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_response_token_await(IntPtr token, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_response_token_free(IntPtr token);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_response_check(byte[] data, UIntPtr len, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_response_num_devices(IntPtr response);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_response_error_codes(IntPtr response);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_response_reply_data_of_len(IntPtr response, UIntPtr device);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_response_reply_data_of(IntPtr response, UIntPtr device);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_response_free(IntPtr response);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_tracing_option_default([Out] byte[] outDefaultFilter, UIntPtr outDefaultFilterLen, out byte outWriter);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_init_tracing(byte[] defaultFilter, byte writer, [Out] byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_tracing_guard_free(IntPtr guard);

        [StructLayout(LayoutKind.Sequential)]
        internal struct FirmwareVersionNative
        {
            public byte CpuMajor, CpuMinor, CpuPatch;
            public byte FpgaMajor, FpgaMinor, FpgaPatch;
            public byte IsEmulator;
            public byte IsSupported;
        }

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_firmware_version_array_len(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_firmware_version_array_get(IntPtr array, UIntPtr index, out FirmwareVersionNative @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_firmware_version_array_free(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_firmware_version_supported_series(out byte major, out byte minor);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_close(ClientHandle client, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_free(IntPtr client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_read_firmware_version(ClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_read_fpga_state(ClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_read_telemetry(ClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_byte_array_len(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_byte_array_data(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_byte_array_free(IntPtr array);

        [StructLayout(LayoutKind.Sequential)]
        internal struct TelemetryNative
        {
            public uint FifoDrop, DispatchError, Processed, SendFailure;
            public ushort Failsafe, PtpUnlockFailsafe;
            public byte SyncResync, BootFailure, CpuWatchdog;
            public uint ExecLate;
        }

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_telemetry_array_len(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_telemetry_array_get(IntPtr array, UIntPtr index, out TelemetryNative @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_telemetry_array_free(IntPtr array);
    }
}
