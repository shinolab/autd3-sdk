using System;
using System.Collections;
using System.Collections.Generic;

namespace AUTD3
{


    internal static class ClientConfigDefaults
    {
        internal static readonly ClientConfig Preset;

        static ClientConfigDefaults()
        {
            var handle = NativeClient.autd3_client_config_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create client config");
            }
            try
            {
                Preset = ClientConfig.FromHandle(handle);
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
        public TimeSpan AckTimeout { get => _ackTimeout ?? ClientConfigDefaults.Preset.AckTimeout; init => _ackTimeout = value; }
        private readonly uint? _maxInflight;
        public uint MaxInflight { get => _maxInflight ?? ClientConfigDefaults.Preset.MaxInflight; init => _maxInflight = value; }

        internal static ClientConfig FromHandle(IntPtr handle)
        {
            var ackTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_client_config_get_ack_timeout_ns);
            OptionNative.Apply("preset", NativeClient.autd3_client_config_get_max_inflight(handle, out var maxInflight));
            return new ClientConfig
            {
                AckTimeout = ackTimeout,
                MaxInflight = (uint)maxInflight,
            };
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
                OptionNative.SetRequiredDuration(handle, "ackTimeout", AckTimeout, NativeClient.autd3_client_config_set_ack_timeout_ns);
                OptionNative.Apply("maxInflight", NativeClient.autd3_client_config_set_max_inflight(handle, (UIntPtr)MaxInflight));
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
                NativeClient.autd3_op_free(op);
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
                NativeClient.autd3_op_free(op);
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

        public bool IsEmpty => Length == 0;

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
