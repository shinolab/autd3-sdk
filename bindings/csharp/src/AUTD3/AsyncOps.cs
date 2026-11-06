using System;
using System.Runtime.InteropServices;
using System.Threading;
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

        internal static async Task<IntPtr> InvokeAsync(Action<IntPtr, CompletionCallback, IntPtr> call, CancellationToken cancellationToken)
        {
            if (!cancellationToken.CanBeCanceled)
            {
                return await InvokeAsync((cb, ud) => call(IntPtr.Zero, cb, ud)).ConfigureAwait(false);
            }
            var token = NativeClient.autd3_cancel_token_new();
            try
            {
                using (cancellationToken.Register(() => NativeClient.autd3_cancel_token_cancel(token)))
                {
                    try
                    {
                        return await InvokeAsync((cb, ud) => call(token, cb, ud)).ConfigureAwait(false);
                    }
                    catch (Autd3Exception e) when (e.Code == Autd3ErrorCode.Aborted && cancellationToken.IsCancellationRequested)
                    {
                        throw new OperationCanceledException(e.Message, e, cancellationToken);
                    }
                }
            }
            finally
            {
                NativeClient.autd3_cancel_token_free(token);
            }
        }

        [MonoPInvokeCallback(typeof(CompletionCallback))]
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
                tcs.SetException(Autd3Exception.FromNative(code, NativeUtil.PtrToString(msg)));
            }
        }
    }
}
