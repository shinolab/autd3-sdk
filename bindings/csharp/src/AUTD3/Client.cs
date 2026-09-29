using System;
using System.Threading;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace AUTD3
{
    public readonly struct FpgaState
    {
        public byte Raw { get; }

        public FpgaState(byte raw)
        {
            Raw = raw;
        }

        public bool IsThermalAsserted => (Raw & (1 << 0)) != 0;
        public bool IsPatternStopped => (Raw & (1 << 4)) != 0;
        public bool IsModStopped => (Raw & (1 << 5)) != 0;
        public bool IsTransitionPending => (Raw & (1 << 6)) != 0;
        public bool ReadsEnabled => (Raw & (1 << 7)) != 0;
    }

    public readonly struct TelemetryCounters : IEquatable<TelemetryCounters>
    {
        public const int Count = 7;

        private readonly uint[] _counters;

        internal TelemetryCounters(uint[] counters)
        {
            _counters = counters;
        }

        public uint this[Telemetry counter] => Get(counter);

        public uint Get(Telemetry counter)
        {
            var index = (int)counter;
            return _counters != null && index < _counters.Length ? _counters[index] : 0;
        }

        public IReadOnlyList<uint> AsArray() => _counters ?? Array.Empty<uint>();

        public bool Equals(TelemetryCounters other)
        {
            var lhs = AsArray();
            var rhs = other.AsArray();
            if (lhs.Count != rhs.Count)
            {
                return false;
            }
            for (var i = 0; i < lhs.Count; i++)
            {
                if (lhs[i] != rhs[i])
                {
                    return false;
                }
            }
            return true;
        }

        public override bool Equals(object? obj) => obj is TelemetryCounters other && Equals(other);

        public override int GetHashCode()
        {
            var hash = new HashCode();
            foreach (var value in AsArray())
            {
                hash.Add(value);
            }
            return hash.ToHashCode();
        }

        public static bool operator ==(TelemetryCounters left, TelemetryCounters right) => left.Equals(right);

        public static bool operator !=(TelemetryCounters left, TelemetryCounters right) => !left.Equals(right);
    }

    public sealed class DeviceStatus
    {
        public IReadOnlyList<DeviceState> Devices { get; }

        internal DeviceStatus(IReadOnlyList<DeviceState> devices)
        {
            Devices = devices;
        }

        public bool AllReady
        {
            get
            {
                foreach (var state in Devices)
                {
                    if (state != DeviceState.Ready)
                    {
                        return false;
                    }
                }
                return true;
            }
        }

        public bool AnyLost
        {
            get
            {
                foreach (var state in Devices)
                {
                    if (state == DeviceState.Lost)
                    {
                        return true;
                    }
                }
                return false;
            }
        }

        public override bool Equals(object? obj)
        {
            if (obj is not DeviceStatus other)
            {
                return false;
            }
            if (Devices.Count != other.Devices.Count)
            {
                return false;
            }
            for (var i = 0; i < Devices.Count; i++)
            {
                if (Devices[i] != other.Devices[i])
                {
                    return false;
                }
            }
            return true;
        }

        public override int GetHashCode()
        {
            var hash = new HashCode();
            foreach (var state in Devices)
            {
                hash.Add(state);
            }
            return hash.ToHashCode();
        }

        public static bool operator ==(DeviceStatus? left, DeviceStatus? right) =>
            left is null ? right is null : left.Equals(right);

        public static bool operator !=(DeviceStatus? left, DeviceStatus? right) => !(left == right);
    }

    public sealed class Checker : IDisposable
    {
        private readonly CheckerHandle _handle;

        internal Checker(IntPtr handle)
        {
            _handle = new CheckerHandle(handle);
        }

        public DeviceStatus Check()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            var status = NativeClient.autd3_checker_check(_handle, err, (UIntPtr)err.Length);
            if (status == IntPtr.Zero)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            try
            {
                var count = (int)NativeClient.autd3_device_status_num_devices(status);
                var devices = new List<DeviceState>(count);
                for (var i = 0; i < count; i++)
                {
                    if (!NativeClient.autd3_device_status_device_state(status, (UIntPtr)i, out var kind))
                    {
                        throw new Autd3Exception("failed to read device state");
                    }
                    devices.Add(DeviceState.FromNative(kind));
                }
                return new DeviceStatus(devices);
            }
            finally
            {
                NativeClient.autd3_device_status_free(status);
            }
        }

        public void Dispose() => _handle.Dispose();
    }

    public sealed class Response
    {
        private readonly byte[] _status;

        internal Response(byte[] status)
        {
            _status = status;
        }

        public IReadOnlyList<byte> Status => _status;

        public void Check()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            if (!NativeClient.autd3_response_check(_status, (UIntPtr)_status.Length, err, (UIntPtr)err.Length))
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
        }
    }

    public sealed class ResponseToken : IDisposable
    {
        private IntPtr _handle;

        internal ResponseToken(IntPtr handle)
        {
            _handle = handle;
        }

        public async Task<Response> AwaitAsync()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("ResponseToken has already been awaited");
            }
            GC.SuppressFinalize(this);
            var data = await Client.ReadByteArrayAsync((cb, ud) =>
                NativeClient.autd3_response_token_await(handle, cb, ud)).ConfigureAwait(false);
            return new Response(data);
        }

        public TaskAwaiter<Response> GetAwaiter() => AwaitAsync().GetAwaiter();

        public void Dispose()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_response_token_free(handle);
            }
            GC.SuppressFinalize(this);
        }

        ~ResponseToken()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_response_token_free(handle);
            }
        }
    }

    public sealed class Client : IDisposable, IAsyncDisposable
    {
        public const int MaxInflight = 127;

        private readonly ClientHandle _handle;
        private readonly Geometry _geometry;

        internal ClientHandle Handle => _handle;

        internal Client(IntPtr handle, Geometry geometry)
        {
            _handle = new ClientHandle(handle);
            _geometry = geometry;
        }

        public static async Task<Client> OpenAsync(Geometry geometry, TransportOption option, ClientConfig config)
        {
            var optionHandle = option.CreateHandle();

            IntPtr configHandle;
            try
            {
                configHandle = config.CreateHandle();
            }
            catch
            {
                NativeClient.autd3_transport_option_free(optionHandle);
                throw;
            }

            Task<IntPtr> task;
            try
            {
                task = AsyncOps.InvokeAsync((cb, ud) =>
                    NativeClient.autd3_client_open(geometry.Handle, optionHandle, configHandle, cb, ud));
            }
            finally
            {
                NativeClient.autd3_client_config_free(configHandle);
            }
            var value = await task.ConfigureAwait(false);
            return new Client(value, geometry);
        }

        public static async Task<(Client Client, Checker Checker)> OpenWithCheckerAsync(Geometry geometry, TransportOption option, ClientConfig config)
        {
            var client = await OpenAsync(geometry, option, config).ConfigureAwait(false);
            var checker = NativeClient.autd3_client_checker(client.Handle);
            if (checker == IntPtr.Zero)
            {
                client.Dispose();
                throw new Autd3Exception("failed to create checker");
            }
            return (client, new Checker(checker));
        }

        public int NumDevices => (int)NativeClient.autd3_client_num_devices(Handle);

        public Geometry Geometry => _geometry;

        public DatagramBuilder DatagramBuilder() => new DatagramBuilder(_geometry, this);

        public Task SendCheckedAsync(Frame frame) =>
            AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send_checked(Handle, frame.Frames.Handle, frame.Index, cb, ud));

        public async Task<ResponseToken> SendAsync(Frame frame)
        {
            var token = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send(Handle, frame.Frames.Handle, frame.Index, cb, ud)).ConfigureAwait(false);
            return new ResponseToken(token);
        }

        public async Task<IReadOnlyList<string>> ReadFirmwareVersionAsync()
        {
            var array = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_read_firmware_version(Handle, cb, ud)).ConfigureAwait(false);
            try
            {
                var count = (int)NativeClient.autd3_string_array_len(array);
                var versions = new List<string>(count);
                for (var i = 0; i < count; i++)
                {
                    versions.Add(NativeUtil.PtrToString(NativeClient.autd3_string_array_get(array, (UIntPtr)i)));
                }
                return versions;
            }
            finally
            {
                NativeClient.autd3_string_array_free(array);
            }
        }

        public async Task<IReadOnlyList<FpgaState>> ReadFpgaStateAsync()
        {
            var bytes = await ReadByteArrayAsync((cb, ud) =>
                NativeClient.autd3_client_read_fpga_state(Handle, cb, ud)).ConfigureAwait(false);
            var states = new FpgaState[bytes.Length];
            for (var i = 0; i < bytes.Length; i++)
            {
                states[i] = new FpgaState(bytes[i]);
            }
            return states;
        }

        public Task<byte[]> ReadErrorDetailAsync() =>
            ReadByteArrayAsync((cb, ud) => NativeClient.autd3_client_read_error_detail(Handle, cb, ud));

        public async Task<IReadOnlyList<TelemetryCounters>> ReadTelemetryAsync()
        {
            var array = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_read_telemetry(Handle, cb, ud)).ConfigureAwait(false);
            try
            {
                var len = (int)NativeClient.autd3_u32_array_len(array);
                var values = new int[len];
                if (len > 0)
                {
                    Marshal.Copy(NativeClient.autd3_u32_array_data(array), values, 0, len);
                }
                var devices = new TelemetryCounters[len / TelemetryCounters.Count];
                for (var d = 0; d < devices.Length; d++)
                {
                    var counters = new uint[TelemetryCounters.Count];
                    for (var i = 0; i < counters.Length; i++)
                    {
                        counters[i] = unchecked((uint)values[d * TelemetryCounters.Count + i]);
                    }
                    devices[d] = new TelemetryCounters(counters);
                }
                return devices;
            }
            finally
            {
                NativeClient.autd3_u32_array_free(array);
            }
        }

        internal static async Task<byte[]> ReadByteArrayAsync(Action<CompletionCallback, IntPtr> invoke)
        {
            var array = await AsyncOps.InvokeAsync(invoke).ConfigureAwait(false);
            try
            {
                var len = (int)NativeClient.autd3_byte_array_len(array);
                var bytes = new byte[len];
                if (len > 0)
                {
                    Marshal.Copy(NativeClient.autd3_byte_array_data(array), bytes, 0, len);
                }
                return bytes;
            }
            finally
            {
                NativeClient.autd3_byte_array_free(array);
            }
        }

        public Task StopAsync() =>
            AsyncOps.InvokeAsync((cb, ud) => NativeClient.autd3_client_stop(Handle, cb, ud));

        public Task CloseAsync() =>
            AsyncOps.InvokeAsync((cb, ud) => NativeClient.autd3_client_close(Handle, cb, ud));

        public void Dispose()
        {
            if (_handle.IsClosed)
            {
                return;
            }
            try
            {
                CloseAsync().GetAwaiter().GetResult();
            }
            finally
            {
                _handle.Dispose();
            }
        }

        public async ValueTask DisposeAsync()
        {
            if (_handle.IsClosed)
            {
                return;
            }
            try
            {
                await CloseAsync().ConfigureAwait(false);
            }
            finally
            {
                _handle.Dispose();
            }
        }
    }
}
