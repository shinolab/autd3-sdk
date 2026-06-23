using System;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace AUTD3
{
    internal static class AsyncOps
    {


        internal static readonly CompletionCallback Callback = OnComplete;

        internal static Task<IntPtr> InvokeAsync(Action<CompletionCallback, IntPtr> call)
        {
            var tcs = new TaskCompletionSource<IntPtr>(TaskCreationOptions.RunContinuationsAsynchronously);
            var handle = GCHandle.Alloc(tcs);
            try
            {
                call(Callback, GCHandle.ToIntPtr(handle));
            }
            catch
            {
                handle.Free();
                throw;
            }
            return tcs.Task;
        }

        private static void OnComplete(int code, IntPtr value, IntPtr msg, IntPtr userData)
        {
            var handle = GCHandle.FromIntPtr(userData);
            var tcs = (TaskCompletionSource<IntPtr>)handle.Target!;
            handle.Free();
            if (code == 0)
            {
                tcs.SetResult(value);
            }
            else
            {
                tcs.SetException(new Autd3Exception(NativeUtil.PtrToString(msg)));
            }
        }
    }
}
