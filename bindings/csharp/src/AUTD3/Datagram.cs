using System;
using System.Collections;
using System.Collections.Generic;

namespace AUTD3
{


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
