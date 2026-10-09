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
        internal static extern IntPtr autd3_transport_option_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_iface(IntPtr option, [MarshalAs(UnmanagedType.LPUTF8Str)] string? interfaceName);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_iface_simulator(IntPtr option);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_iface_addr(IntPtr option, [MarshalAs(UnmanagedType.LPUTF8Str)] string addr);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_iface_addr(IntPtr option, out IntPtr addr);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_heartbeat(IntPtr option, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_heartbeat(IntPtr option, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_timer_resolution(IntPtr option, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_timer_resolution(IntPtr option, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_send_buffer(IntPtr option, ulong bytes);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_send_buffer(IntPtr option, out ulong bytes);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_send_rate_limit(IntPtr option, float percent);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_send_rate_limit(IntPtr option, out float percent);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_reply_timeout(IntPtr option, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_reply_timeout(IntPtr option, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_lost_timeout(IntPtr option, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_lost_timeout(IntPtr option, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_response_timeout(IntPtr option, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_response_timeout(IntPtr option, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_enumeration_timeout(IntPtr option, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_enumeration_timeout(IntPtr option, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_set_sync_timeout(IntPtr option, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_transport_option_get_sync_timeout(IntPtr option, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_transport_option_free(IntPtr option);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_udp_free_string(IntPtr ptr);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_udp_emulator_spawn(UIntPtr numDevices, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_udp_emulator_option(UdpEmulatorHandle emulator);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_udp_emulator_reboot(UdpEmulatorHandle emulator, UIntPtr index);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_udp_emulator_free(IntPtr emulator);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_client_config_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_require_supported_firmware(IntPtr config, [MarshalAs(UnmanagedType.I1)] bool value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_ack_timeout_ns(IntPtr config, ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_max_inflight(IntPtr config, UIntPtr value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_max_resync_rounds(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_get_ack_timeout_ns(IntPtr config, out ulong ns);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_get_max_inflight(IntPtr config, out UIntPtr value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_get_max_resync_rounds(IntPtr config, out uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_get_require_supported_firmware(IntPtr config, [MarshalAs(UnmanagedType.I1)] out bool value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_config_free(IntPtr config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_params_max_devices();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_params_pwe_table_size();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_telemetry_count();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_telemetry_all([Out] byte[] dst, UIntPtr len);

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
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_fpga_state_is_failsafe_active(byte raw);


        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_command_each(IntPtr[] ops, UIntPtr numDevices);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_command_sequence(IntPtr[] ops, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_op_free(IntPtr op);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_frames_encode(GeometryHandle geometry, IntPtr command, out int outCode, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_frames_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_frames_encode_into(FramesHandle frames, GeometryHandle geometry, IntPtr command, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_frames_num_frames(FramesHandle frames);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_frames_free(IntPtr frames);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_open(GeometryHandle geometry, IntPtr option, IntPtr config, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_client_state_checker(ClientHandle client);








        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_client_num_devices(ClientHandle client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_device_time_now(ClientHandle client, out ulong ns, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send_checked(ClientHandle client, FramesHandle frames, long frame, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send(ClientHandle client, FramesHandle frames, long frame, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send_streaming(ClientHandle client, IntPtr command, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_stream_token_await(IntPtr token, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_stream_token_free(IntPtr token);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_response_token_await(IntPtr token, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_response_token_free(IntPtr token);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_response_check(byte[] data, UIntPtr len, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_response_num_devices(IntPtr response);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_response_status(IntPtr response);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_response_value_len(IntPtr response, UIntPtr device);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_response_value_data(IntPtr response, UIntPtr device);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_response_free(IntPtr response);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_client_bus_stats(ClientHandle client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong autd3_bus_stats_frames(BusStatsHandle stats);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong autd3_bus_stats_resets(BusStatsHandle stats);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong autd3_bus_stats_heartbeats(BusStatsHandle stats);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong autd3_bus_stats_missed_replies(BusStatsHandle stats);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong autd3_bus_stats_acked_frames(BusStatsHandle stats);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong autd3_bus_stats_worst_ack_latency_ns(BusStatsHandle stats);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong autd3_bus_stats_mean_ack_latency_ns(BusStatsHandle stats);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_bus_stats_free(IntPtr stats);

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
        internal static extern void autd3_client_silent_stop(ClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_close(ClientHandle client, CompletionCallback cb, IntPtr userData);

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

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_u32_array_len(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_u32_array_data(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_u32_array_free(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_checker_check(CheckerHandle checker, out int code, byte[] err, UIntPtr errLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_checker_free(IntPtr checker);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_device_status_num_devices(IntPtr status);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_device_status_device_state(IntPtr status, UIntPtr index, out byte outKind);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_device_status_free(IntPtr status);
    }
}
