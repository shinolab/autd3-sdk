using System;
using System.Collections;
using System.Collections.Generic;

namespace AUTD3
{


    internal static class ClientConfigDefaults
    {
        internal static readonly TimeSpan AckTimeout;
        internal static readonly uint MaxInflight;
        internal static readonly uint MaxResyncRounds;
        internal static readonly bool RequireSupportedFirmware;

        static ClientConfigDefaults()
        {
            var handle = NativeClient.autd3_client_config_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create client config");
            }
            try
            {
                AckTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_client_config_get_ack_timeout_ns);
                OptionNative.Apply("preset", NativeClient.autd3_client_config_get_max_inflight(handle, out var maxInflight));
                MaxInflight = (uint)maxInflight;
                OptionNative.Apply("preset", NativeClient.autd3_client_config_get_max_resync_rounds(handle, out MaxResyncRounds));
                OptionNative.Apply("preset", NativeClient.autd3_client_config_get_require_supported_firmware(handle, out RequireSupportedFirmware));
            }
            finally
            {
                NativeClient.autd3_client_config_free(handle);
            }
        }
    }

    public readonly struct ClientConfig
    {
        private readonly TimeSpan? _ackTimeout;
        public TimeSpan AckTimeout { get => _ackTimeout ?? ClientConfigDefaults.AckTimeout; init => _ackTimeout = value; }
        private readonly uint? _maxInflight;
        public uint MaxInflight { get => _maxInflight ?? ClientConfigDefaults.MaxInflight; init => _maxInflight = value; }
        private readonly uint? _maxResyncRounds;
        public uint MaxResyncRounds { get => _maxResyncRounds ?? ClientConfigDefaults.MaxResyncRounds; init => _maxResyncRounds = value; }
        private readonly bool? _requireSupportedFirmware;
        public bool RequireSupportedFirmware { get => _requireSupportedFirmware ?? ClientConfigDefaults.RequireSupportedFirmware; init => _requireSupportedFirmware = value; }

        internal IntPtr CreateHandle()
        {
            var handle = NativeClient.autd3_client_config_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create client config");
            }
            try
            {
                OptionNative.SetRequiredDuration(handle, "ackTimeout", AckTimeout, NativeClient.autd3_client_config_set_ack_timeout_ns);
                OptionNative.Apply("maxInflight", NativeClient.autd3_client_config_set_max_inflight(handle, (UIntPtr)MaxInflight));
                OptionNative.Apply("maxResyncRounds", NativeClient.autd3_client_config_set_max_resync_rounds(handle, MaxResyncRounds));
                OptionNative.Apply("requireSupportedFirmware", NativeClient.autd3_client_config_set_require_supported_firmware(handle, RequireSupportedFirmware));
            }
            catch
            {
                NativeClient.autd3_client_config_free(handle);
                throw;
            }
            return handle;
        }
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

        public Frames() : this(NativeClient.autd3_frames_new())
        {
        }

        public static Frames Encode(Geometry geometry, ICommand command)
        {
            if (geometry == null)
            {
                throw new ArgumentNullException(nameof(geometry));
            }
            if (command == null)
            {
                throw new ArgumentNullException(nameof(command));
            }
            var op = CreateOp(geometry, command);
            var err = new byte[NativeAbi.ErrorBufferLength];
            IntPtr handle;
            int code;
            try
            {
                handle = NativeClient.autd3_frames_encode(geometry.Handle, op, out code, err, (UIntPtr)err.Length);
            }
            catch
            {
                NativeClient.autd3_op_free(op);
                throw;
            }
            if (handle == IntPtr.Zero)
            {
                throw Autd3Exception.FromNative(code, err);
            }
            return new Frames(handle);
        }

        public void EncodeInto(Geometry geometry, ICommand command)
        {
            if (geometry == null)
            {
                throw new ArgumentNullException(nameof(geometry));
            }
            if (command == null)
            {
                throw new ArgumentNullException(nameof(command));
            }
            var op = CreateOp(geometry, command);
            var err = new byte[NativeAbi.ErrorBufferLength];
            int code;
            try
            {
                code = NativeClient.autd3_frames_encode_into(Handle, geometry.Handle, op, err, (UIntPtr)err.Length);
            }
            catch
            {
                NativeClient.autd3_op_free(op);
                throw;
            }
            if (code != 0)
            {
                throw Autd3Exception.FromNative(code, err);
            }
        }

        private static IntPtr CreateOp(Geometry geometry, ICommand command)
        {
            var op = command.CreateOp(geometry);
            if (op == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create the command", Autd3ErrorCode.InvalidArgument);
            }
            return op;
        }

        public int Length => (int)NativeClient.autd3_frames_num_frames(Handle);

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
