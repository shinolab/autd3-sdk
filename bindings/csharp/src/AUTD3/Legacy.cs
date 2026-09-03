using System;
using System.Collections;
using System.Collections.Generic;
using System.Threading.Tasks;

namespace AUTD3.Legacy
{


    public readonly struct LegacyClientConfig
    {
        public uint TimeoutCycles { get; }
        public byte? RtPriority { get; }
        public bool DisableRtPriority { get; }
        public RtSchedulePolicy RtPolicy { get; }
        public ulong? RtAffinity { get; }

        public LegacyClientConfig() : this(timeoutCycles: 2000)
        {
        }

        public LegacyClientConfig(
            uint timeoutCycles = 2000,
            byte? rtPriority = null,
            bool disableRtPriority = false,
            RtSchedulePolicy rtPolicy = RtSchedulePolicy.Fifo,
            ulong? rtAffinity = null)
        {
            if (rtPriority.HasValue && disableRtPriority)
            {
                throw new ArgumentException("rtPriority and disableRtPriority are mutually exclusive");
            }
            TimeoutCycles = timeoutCycles;
            RtPriority = rtPriority;
            DisableRtPriority = disableRtPriority;
            RtPolicy = rtPolicy;
            RtAffinity = rtAffinity;
        }

        internal IntPtr CreateHandle()
        {
            var handle = NativeLegacyClient.autd3_legacy_client_config_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create legacy client config");
            }
            try
            {
                NativeConfig.Apply("timeoutCycles", NativeLegacyClient.autd3_legacy_client_config_set_timeout_cycles(handle, TimeoutCycles));
                NativeConfig.Apply("rtPriority", NativeLegacyClient.autd3_legacy_client_config_set_rt_priority(handle, NativeConfig.RtPriorityMode(RtPriority, DisableRtPriority), RtPriority ?? 0));
                NativeConfig.Apply("rtPolicy", NativeLegacyClient.autd3_legacy_client_config_set_rt_policy(handle, (byte)RtPolicy));
                NativeConfig.Apply("rtAffinity", NativeLegacyClient.autd3_legacy_client_config_set_rt_affinity(handle, RtAffinity.HasValue, (UIntPtr)(RtAffinity ?? 0)));
            }
            catch
            {
                NativeLegacyClient.autd3_legacy_client_config_free(handle);
                throw;
            }
            return handle;
        }
    }

    public interface ILegacyCommand
    {
        internal IntPtr CreateOp();
    }

    public sealed class LegacyChangePatternBank : ILegacyCommand
    {
        private readonly byte _kind;
        private readonly PatternBank _bank;
        private readonly TransitionMode _transitionMode;

        private LegacyChangePatternBank(byte kind, PatternBank bank, TransitionMode transitionMode)
        {
            _kind = kind;
            _bank = bank;
            _transitionMode = transitionMode;
        }

        public static LegacyChangePatternBank Pattern(PatternBank bank) =>
            new LegacyChangePatternBank(0, bank, TransitionMode.Immediate);

        public static LegacyChangePatternBank FociStm(PatternBank bank, TransitionMode? transitionMode = null) =>
            new LegacyChangePatternBank(1, bank, transitionMode ?? TransitionMode.Immediate);

        public static LegacyChangePatternBank PatternStm(PatternBank bank, TransitionMode? transitionMode = null) =>
            new LegacyChangePatternBank(2, bank, transitionMode ?? TransitionMode.Immediate);

        IntPtr ILegacyCommand.CreateOp() =>
            NativeLegacyClient.autd3_legacy_op_change_segment(_kind, (byte)_bank, _transitionMode.Mode, _transitionMode.Value);
    }

    public sealed class LegacyDatagramBuilder : IDisposable
    {
        private readonly Geometry _geometry;
        private readonly int _numDevices;
        private readonly LegacyClient? _client;

        private readonly LegacyDatagramBuilderHandle _handle;

        internal LegacyDatagramBuilderHandle Handle => _handle;

        public LegacyDatagramBuilder(Geometry geometry) : this(geometry, null)
        {
        }

        internal LegacyDatagramBuilder(Geometry geometry, LegacyClient? client)
        {
            _geometry = geometry;
            _numDevices = geometry.NumDevices;
            _client = client;
            var handle = NativeLegacyClient.autd3_legacy_datagram_builder_new(geometry.Handle);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create legacy datagram builder");
            }
            _handle = new LegacyDatagramBuilderHandle(handle);
        }

        public LegacyDatagramBuilder Push(ICommand command)
        {
            var op = command.CreateOp();
            if (NativeLegacyClient.autd3_legacy_datagram_builder_push(Handle, op) != 0)
            {
                throw new Autd3Exception("failed to push the command onto the legacy datagram builder");
            }
            return this;
        }

        public LegacyDatagramBuilder Push(ILegacyCommand command)
        {
            var op = command.CreateOp();
            if (NativeLegacyClient.autd3_legacy_datagram_builder_push_legacy(Handle, op) != 0)
            {
                throw new Autd3Exception("failed to push the command onto the legacy datagram builder");
            }
            return this;
        }

        public LegacyDatagramBuilder PushEach(Func<Device, ICommand?> factory)
        {
            var ops = new IntPtr[_numDevices];
            try
            {
                for (var i = 0; i < _numDevices; i++)
                {
                    var command = factory(_geometry[i]);
                    ops[i] = command == null ? IntPtr.Zero : command.CreateOp();
                }
            }
            catch
            {
                foreach (var op in ops)
                {
                    if (op != IntPtr.Zero)
                    {
                        NativeClient.autd3_op_free(op);
                    }
                }
                throw;
            }
            if (NativeLegacyClient.autd3_legacy_datagram_builder_push_each(Handle, ops, (UIntPtr)_numDevices) != 0)
            {
                throw new Autd3Exception("failed to push the per-device commands onto the legacy datagram builder");
            }
            return this;
        }

        public LegacyFrames Build()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            using var client = new HandleLease(_client?.Handle);
            var handle = NativeLegacyClient.autd3_legacy_datagram_builder_build(Handle, client.Pointer, err, (UIntPtr)err.Length);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            return new LegacyFrames(handle);
        }

        public void Dispose() => _handle.Dispose();
    }

    public readonly struct LegacyFrame
    {
        internal LegacyFrames Frames { get; }
        internal long Index { get; }

        internal LegacyFrame(LegacyFrames frames, long index)
        {
            Frames = frames;
            Index = index;
        }
    }

    public sealed class LegacyFrames : IDisposable, IEnumerable<LegacyFrame>
    {
        private readonly LegacyFramesHandle _handle;

        internal LegacyFramesHandle Handle => _handle;

        internal LegacyFrames(IntPtr handle)
        {
            _handle = new LegacyFramesHandle(handle);
        }

        public int Length => (int)NativeLegacyClient.autd3_legacy_frames_num_frames(Handle);

        public LegacyFrame this[int index]
        {
            get
            {
                if (index < 0 || index >= Length)
                {
                    throw new ArgumentOutOfRangeException(nameof(index));
                }
                return new LegacyFrame(this, index);
            }
        }

        public IEnumerator<LegacyFrame> GetEnumerator()
        {
            var count = Length;
            for (long i = 0; i < count; i++)
            {
                yield return new LegacyFrame(this, i);
            }
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        public void Dispose() => _handle.Dispose();
    }

    public sealed class LegacyClient : IDisposable, IAsyncDisposable
    {
        private readonly LegacyClientHandle _handle;
        private readonly Geometry _geometry;

        internal LegacyClientHandle Handle => _handle;

        private LegacyClient(IntPtr handle, Geometry geometry)
        {
            _handle = new LegacyClientHandle(handle);
            _geometry = geometry;
        }

        public static async Task<LegacyClient> OpenAsync(Geometry geometry, ILegacyLink link, LegacyClientConfig config)
        {
            var opener = link.TakeLegacyOpener();

            IntPtr configHandle;
            try
            {
                configHandle = config.CreateHandle();
            }
            catch
            {
                NativeLegacyClient.autd3_legacy_client_opener_free(opener);
                throw;
            }

            Task<IntPtr> task;
            try
            {
                task = AsyncOps.InvokeAsync((cb, ud) =>
                    NativeLegacyClient.autd3_legacy_client_open(geometry.Handle, opener, configHandle, cb, ud));
            }
            finally
            {
                NativeLegacyClient.autd3_legacy_client_config_free(configHandle);
            }
            var value = await task.ConfigureAwait(false);
            return new LegacyClient(value, geometry);
        }

        public static async Task<(LegacyClient Client, Checker Checker)> OpenWithCheckerAsync(Geometry geometry, ILegacyLink link, LegacyClientConfig config)
        {
            var client = await OpenAsync(geometry, link, config).ConfigureAwait(false);
            var checker = NativeLegacyClient.autd3_legacy_client_checker(client.Handle);
            if (checker == IntPtr.Zero)
            {
                client.Dispose();
                throw new Autd3Exception("failed to create checker");
            }
            return (client, new Checker(checker));
        }

        public int NumDevices => (int)NativeLegacyClient.autd3_legacy_client_num_devices(Handle);

        public LegacyDatagramBuilder DatagramBuilder() => new LegacyDatagramBuilder(_geometry, this);

        public Task SendCheckedAsync(LegacyFrame frame) =>
            AsyncOps.InvokeAsync((cb, ud) =>
                NativeLegacyClient.autd3_legacy_client_send_checked(Handle, frame.Frames.Handle, frame.Index, cb, ud));

        public Task<byte[]> SendAsync(LegacyFrame frame) =>
            Client.ReadByteArrayAsync((cb, ud) =>
                NativeLegacyClient.autd3_legacy_client_send(Handle, frame.Frames.Handle, frame.Index, cb, ud));

        public async Task<IReadOnlyList<string>> ReadFirmwareVersionAsync()
        {
            var array = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeLegacyClient.autd3_legacy_client_read_firmware_version(Handle, cb, ud)).ConfigureAwait(false);
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
            var bytes = await Client.ReadByteArrayAsync((cb, ud) =>
                NativeLegacyClient.autd3_legacy_client_read_fpga_state(Handle, cb, ud)).ConfigureAwait(false);
            var states = new FpgaState[bytes.Length];
            for (var i = 0; i < bytes.Length; i++)
            {
                states[i] = new FpgaState(bytes[i]);
            }
            return states;
        }

        public Task StopAsync() =>
            AsyncOps.InvokeAsync((cb, ud) => NativeLegacyClient.autd3_legacy_client_stop(Handle, cb, ud));

        public Task CloseAsync() =>
            AsyncOps.InvokeAsync((cb, ud) => NativeLegacyClient.autd3_legacy_client_close(Handle, cb, ud));

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
