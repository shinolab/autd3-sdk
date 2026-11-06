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
        internal static extern UIntPtr autd3_client_num_devices(ClientHandle client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_device_time_now(ClientHandle client, out ulong ns, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_cancel_token_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_cancel_token_cancel(IntPtr token);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_cancel_token_free(IntPtr token);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_discard_unread_replies(ClientHandle client, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_send_heartbeat(ClientHandle client, out ushort msgId, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send(ClientHandle client, FramesHandle frames, IntPtr cancel, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send_frame(ClientHandle client, FramesHandle frames, UIntPtr frame, ExecAtNative exec, IntPtr cancel, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_recv(ClientHandle client, IntPtr cancel, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_collect(ClientHandle client, ushort msgId, IntPtr cancel, CompletionCallback cb, IntPtr userData);

        [StructLayout(LayoutKind.Sequential)]
        internal struct DeviceReplyNative
        {
            public UIntPtr Device;
            public ushort MsgId;
            public byte Status;
            public byte Flags;
            public byte UnitId;
            public ulong SysTime;
            public TelemetryNative Telemetry;
            public byte FpgaState;
        }

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_device_reply_get(IntPtr reply, out DeviceReplyNative @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_device_reply_data_len(IntPtr reply);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_device_reply_data(IntPtr reply);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_device_reply_free(IntPtr reply);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_response_num_devices(IntPtr response);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_response_reply(IntPtr response, UIntPtr device);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_response_device_status(IntPtr response, [Out] DeviceStatusNative[] status, UIntPtr len, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_response_check(IntPtr response, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_response_free(IntPtr response);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_inflight_frames_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_inflight_frames_push(InflightFramesHandle inflight, ushort msgId);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_inflight_frames_len(InflightFramesHandle inflight, out UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_inflight_frames_collect_oldest(InflightFramesHandle inflight, ClientHandle client, IntPtr cancel, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_inflight_frames_free(IntPtr inflight);

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
        internal static extern void autd3_client_free(IntPtr client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_read_firmware_version(ClientHandle client, IntPtr cancel, CompletionCallback cb, IntPtr userData);

        [StructLayout(LayoutKind.Sequential)]
        internal struct TelemetryNative
        {
            public uint FifoDrop, ExecLate;
            public byte SyncResync, CpuWatchdog, BootFailure;
        }
    }
}
