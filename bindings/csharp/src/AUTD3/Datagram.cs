using System;
using System.Collections.Generic;

namespace AUTD3
{


    public sealed class Frames : IDisposable
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
            var frames = new Frames();
            try
            {
                frames.EncodeInto(geometry, command);
            }
            catch
            {
                frames.Dispose();
                throw;
            }
            return frames;
        }

        public static Frames Encode(Geometry geometry, IEnumerable<ICommand> commands) =>
            Encode(geometry, SequenceCommand.Of(commands));

        public void EncodeInto(Geometry geometry, IEnumerable<ICommand> commands) =>
            EncodeInto(geometry, SequenceCommand.Of(commands));

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

        public void Dispose() => _handle.Dispose();
    }
}
