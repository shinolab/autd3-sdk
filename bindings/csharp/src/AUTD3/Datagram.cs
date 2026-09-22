using System;
using System.Collections;
using System.Collections.Generic;

namespace AUTD3
{


    public enum RtSchedulePolicy : byte
    {
        Normal = 0,
        Fifo = 1,
        RoundRobin = 2,
    }

    public readonly struct RtPriority : IEquatable<RtPriority>
    {
        private const byte ModeDefault = 0;
        private const byte ModeDisabled = 1;
        private const byte ModeExplicit = 2;
        private const byte ModeMin = 3;
        private const byte ModeMax = 4;

        internal byte Mode { get; }
        internal byte Value { get; }

        private RtPriority(byte mode, byte value)
        {
            Mode = mode;
            Value = value;
        }

        public RtPriority(byte value) : this(ModeExplicit, value)
        {
        }

        public static RtPriority Default => default;
        public static RtPriority Disabled => new RtPriority(ModeDisabled, 0);
        public static RtPriority Min => new RtPriority(ModeMin, 0);
        public static RtPriority Max => new RtPriority(ModeMax, 0);

        public bool Equals(RtPriority other) => Mode == other.Mode && Value == other.Value;
        public override bool Equals(object? obj) => obj is RtPriority other && Equals(other);
        public override int GetHashCode() => HashCode.Combine(Mode, Value);
        public static bool operator ==(RtPriority left, RtPriority right) => left.Equals(right);
        public static bool operator !=(RtPriority left, RtPriority right) => !left.Equals(right);

        public override string ToString() => Mode switch
        {
            ModeDisabled => "RtPriority.Disabled",
            ModeExplicit => $"RtPriority({Value})",
            ModeMin => "RtPriority.Min",
            ModeMax => "RtPriority.Max",
            _ => "RtPriority.Default",
        };
    }

    public readonly struct ClientConfig
    {
        public bool LowLatency { get; }
        public uint TimeoutCycles { get; }
        public uint MaxInflight { get; }
        public uint MaxResyncRounds { get; }
        public uint ResetResendCycles { get; }
        public RtPriority RtPriority { get; }
        public RtSchedulePolicy RtPolicy { get; }
        public ulong? RtAffinity { get; }
        public bool ValidateState { get; }
        public bool RequireSupportedFirmware { get; }

        public ClientConfig() : this(lowLatency: false)
        {
        }

        public ClientConfig(
            bool lowLatency = false,
            uint timeoutCycles = 10,
            uint maxInflight = 127,
            uint maxResyncRounds = 8,
            uint resetResendCycles = 2,
            RtPriority rtPriority = default,
            RtSchedulePolicy rtPolicy = RtSchedulePolicy.Fifo,
            ulong? rtAffinity = null,
            bool validateState = true,
            bool requireSupportedFirmware = false)
        {
            LowLatency = lowLatency;
            TimeoutCycles = timeoutCycles;
            MaxInflight = maxInflight;
            MaxResyncRounds = maxResyncRounds;
            ResetResendCycles = resetResendCycles;
            RtPriority = rtPriority;
            RtPolicy = rtPolicy;
            RtAffinity = rtAffinity;
            ValidateState = validateState;
            RequireSupportedFirmware = requireSupportedFirmware;
        }

        internal IntPtr CreateHandle()
        {
            var handle = NativeClient.autd3_client_config_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create client config");
            }
            try
            {
                NativeConfig.Apply("lowLatency", NativeClient.autd3_client_config_set_low_latency(handle, LowLatency));
                NativeConfig.Apply("timeoutCycles", NativeClient.autd3_client_config_set_timeout_cycles(handle, TimeoutCycles));
                NativeConfig.Apply("maxInflight", NativeClient.autd3_client_config_set_max_inflight(handle, (UIntPtr)MaxInflight));
                NativeConfig.Apply("maxResyncRounds", NativeClient.autd3_client_config_set_max_resync_rounds(handle, MaxResyncRounds));
                NativeConfig.Apply("resetResendCycles", NativeClient.autd3_client_config_set_reset_resend_cycles(handle, ResetResendCycles));
                NativeConfig.Apply("rtPriority", NativeClient.autd3_client_config_set_rt_priority(handle, RtPriority.Mode, RtPriority.Value));
                NativeConfig.Apply("rtPolicy", NativeClient.autd3_client_config_set_rt_policy(handle, (byte)RtPolicy));
                NativeConfig.Apply("rtAffinity", NativeClient.autd3_client_config_set_rt_affinity(handle, RtAffinity.HasValue, (UIntPtr)(RtAffinity ?? 0)));
                NativeConfig.Apply("validateState", NativeClient.autd3_client_config_set_validate_state(handle, ValidateState));
                NativeConfig.Apply("requireSupportedFirmware", NativeClient.autd3_client_config_set_require_supported_firmware(handle, RequireSupportedFirmware));
            }
            catch
            {
                NativeClient.autd3_client_config_free(handle);
                throw;
            }
            return handle;
        }
    }

    internal static class NativeConfig
    {
        internal static void Apply(string field, int code)
        {
            if (code != 0)
            {
                throw new Autd3Exception($"`{field}` is out of the range the native library accepts");
            }
        }
    }

    public sealed class DatagramBuilder : IDisposable
    {
        private readonly Geometry _geometry;
        private readonly int _numDevices;
        private readonly Client? _client;

        private readonly DatagramBuilderHandle _handle;

        internal DatagramBuilderHandle Handle => _handle;

        public DatagramBuilder(Geometry geometry) : this(geometry, null)
        {
        }

        internal DatagramBuilder(Geometry geometry, Client? client)
        {
            _geometry = geometry;
            _numDevices = geometry.NumDevices;
            _client = client;
            var handle = NativeClient.autd3_datagram_builder_new(geometry.Handle);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create datagram builder");
            }
            _handle = new DatagramBuilderHandle(handle);
        }

        public DatagramBuilder Push(ICommand command)
        {
            var op = command.CreateOp();
            if (NativeClient.autd3_datagram_builder_push(Handle, op) != 0)
            {
                throw new Autd3Exception("failed to push the command onto the datagram builder");
            }
            return this;
        }

        public DatagramBuilder PushEach(Func<Device, ICommand?> factory)
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
            if (NativeClient.autd3_datagram_builder_push_each(Handle, ops, (UIntPtr)_numDevices) != 0)
            {
                throw new Autd3Exception("failed to push the per-device commands onto the datagram builder");
            }
            return this;
        }

        public Frames Build()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            using var client = new HandleLease(_client?.Handle);
            var handle = NativeClient.autd3_datagram_builder_build(Handle, client.Pointer, err, (UIntPtr)err.Length);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            return new Frames(handle);
        }

        public void Dispose() => _handle.Dispose();
    }


    public readonly struct Frame
    {
        internal Frames Frames { get; }
        internal long Index { get; }

        internal Frame(Frames frames, long index)
        {
            Frames = frames;
            Index = index;
        }
    }

    public sealed class Frames : IDisposable, IEnumerable<Frame>
    {
        private readonly FramesHandle _handle;

        internal FramesHandle Handle => _handle;

        internal Frames(IntPtr handle)
        {
            _handle = new FramesHandle(handle);
        }

        public int Length => (int)NativeClient.autd3_datagrams_num_frames(Handle);

        public Frame this[int index]
        {
            get
            {
                if (index < 0 || index >= Length)
                {
                    throw new ArgumentOutOfRangeException(nameof(index));
                }
                return new Frame(this, index);
            }
        }

        public IEnumerator<Frame> GetEnumerator()
        {
            var count = Length;
            for (long i = 0; i < count; i++)
            {
                yield return new Frame(this, i);
            }
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        public void Dispose() => _handle.Dispose();
    }
}
