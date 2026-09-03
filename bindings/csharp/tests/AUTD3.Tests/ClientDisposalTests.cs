using System;
using System.Collections.Concurrent;
using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3.Legacy;
using Xunit;

namespace AUTD3.Tests
{
    public class ClientDisposalTests
    {
        private sealed class Marker : Exception
        {
        }

        private sealed class NonPumpingContext : SynchronizationContext
        {
            private readonly ConcurrentQueue<(SendOrPostCallback, object?)> _posted = new ConcurrentQueue<(SendOrPostCallback, object?)>();

            public override void Post(SendOrPostCallback d, object? state) => _posted.Enqueue((d, state));

            public override void Send(SendOrPostCallback d, object? state) => _posted.Enqueue((d, state));
        }

        private static Geometry SingleDevice() => new Geometry(new[] { new Autd3(Vector3.Zero) });

        [Fact]
        public async Task AwaitUsingReleasesTheClient()
        {
            using var geometry = SingleDevice();
            var client = await Client.OpenAsync(geometry, new AUTD3.Link.Nop(), new ClientConfig());
            await using (client)
            {
                Assert.Equal(1, client.NumDevices);
            }
            Assert.Throws<ObjectDisposedException>(() => client.NumDevices);
        }

        [Fact]
        public async Task AwaitUsingReleasesTheClientOnException()
        {
            using var geometry = SingleDevice();
            var client = await Client.OpenAsync(geometry, new AUTD3.Link.Nop(), new ClientConfig());
            await Assert.ThrowsAsync<Marker>(async () =>
            {
                await using (client)
                {
                    throw new Marker();
                }
            });
            Assert.Throws<ObjectDisposedException>(() => client.NumDevices);
        }

        [Fact]
        public async Task ExplicitCloseInsideAwaitUsingIsSafe()
        {
            using var geometry = SingleDevice();
            await using var client = await Client.OpenAsync(geometry, new AUTD3.Link.Nop(), new ClientConfig());
            await client.CloseAsync();
        }

        [Fact]
        public async Task DisposingTwiceIsSafe()
        {
            using var geometry = SingleDevice();
            var client = await Client.OpenAsync(geometry, new AUTD3.Link.Nop(), new ClientConfig());
            await client.DisposeAsync();
            await client.DisposeAsync();
            client.Dispose();
        }

        [Fact]
        public async Task SyncDisposeDoesNotDeadlockOnASynchronizationContext()
        {
            var tcs = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            var thread = new Thread(() =>
            {
                try
                {
                    SynchronizationContext.SetSynchronizationContext(new NonPumpingContext());
                    using var geometry = SingleDevice();
                    var client = Client.OpenAsync(geometry, new AUTD3.Link.Nop(), new ClientConfig()).GetAwaiter().GetResult();
                    client.Dispose();
                    Assert.Throws<ObjectDisposedException>(() => client.NumDevices);
                    tcs.SetResult(true);
                }
                catch (Exception e)
                {
                    tcs.SetException(e);
                }
            })
            { IsBackground = true };
            thread.Start();

            var finished = await Task.WhenAny(tcs.Task, Task.Delay(TimeSpan.FromSeconds(10)));
            Assert.Same(tcs.Task, finished);
            Assert.True(await tcs.Task);
        }

        [Fact]
        public async Task LegacyClientSupportsAwaitUsing()
        {
            using var geometry = SingleDevice();
            var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());
            await using (client)
            {
                Assert.Equal(1, client.NumDevices);
            }
            Assert.Throws<ObjectDisposedException>(() => client.NumDevices);
        }
    }
}
