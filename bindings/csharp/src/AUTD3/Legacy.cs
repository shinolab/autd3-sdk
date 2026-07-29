using System;
using System.Collections;
using System.Collections.Generic;
using System.Threading.Tasks;

namespace AUTD3.Legacy
{


    public readonly struct LegacyClientConfig
    {
        public uint TimeoutCycles { get; }

        public LegacyClientConfig() : this(timeoutCycles: 2000)
        {
        }

        public LegacyClientConfig(uint timeoutCycles = 2000)
        {
            TimeoutCycles = timeoutCycles;
        }

        internal IntPtr CreateHandle()
        {
            var handle = NativeLegacyClient.autd3_legacy_client_config_new(TimeoutCycles);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create legacy client config");
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

        internal IntPtr Handle { get; private set; }

        public LegacyDatagramBuilder(Geometry geometry)
        {
            _geometry = geometry;
            _numDevices = geometry.NumDevices;
            Handle = NativeLegacyClient.autd3_legacy_datagram_builder_new(geometry.Handle);
            if (Handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create legacy datagram builder");
            }
        }

        public LegacyDatagramBuilder Push(ICommand command)
        {
            var op = command.CreateOp();
            NativeLegacyClient.autd3_legacy_datagram_builder_push(Handle, op);
            return this;
        }

        public LegacyDatagramBuilder Push(ILegacyCommand command)
        {
            var op = command.CreateOp();
            NativeLegacyClient.autd3_legacy_datagram_builder_push_legacy(Handle, op);
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
            NativeLegacyClient.autd3_legacy_datagram_builder_push_each(Handle, ops, (UIntPtr)_numDevices);
            return this;
        }

        public LegacyFrames Build()
        {
            var err = new byte[256];
            var handle = NativeLegacyClient.autd3_legacy_datagram_builder_build(Handle, err, (UIntPtr)err.Length);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            return new LegacyFrames(handle);
        }

        public void Dispose()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeLegacyClient.autd3_legacy_datagram_builder_free(Handle);
                Handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~LegacyDatagramBuilder()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeLegacyClient.autd3_legacy_datagram_builder_free(Handle);
            }
        }
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
        internal IntPtr Handle { get; private set; }

        internal LegacyFrames(IntPtr handle)
        {
            Handle = handle;
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

        public void Dispose()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeLegacyClient.autd3_legacy_frames_free(Handle);
                Handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~LegacyFrames()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeLegacyClient.autd3_legacy_frames_free(Handle);
            }
        }
    }

    public sealed class LegacyClient : IDisposable
    {
        internal IntPtr Handle { get; private set; }

        private readonly Geometry _geometry;

        private LegacyClient(IntPtr handle, Geometry geometry)
        {
            Handle = handle;
            _geometry = geometry;
        }

        public static async Task<LegacyClient> OpenAsync(Geometry geometry, ILegacyLink link, LegacyClientConfig config)
        {
            var opener = link.TakeLegacyOpener();


            var configHandle = config.CreateHandle();
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

        public LegacyDatagramBuilder DatagramBuilder() => new LegacyDatagramBuilder(_geometry);

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
            if (Handle != IntPtr.Zero)
            {
                NativeLegacyClient.autd3_legacy_client_free(Handle);
                Handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~LegacyClient()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeLegacyClient.autd3_legacy_client_free(Handle);
            }
        }
    }
}
