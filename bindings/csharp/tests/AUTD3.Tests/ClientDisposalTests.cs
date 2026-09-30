using System;
using System.Collections.Concurrent;
using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
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
            using var emulator = new UdpEmulator(1);
            var client = await Client.OpenAsync(geometry, Driven.Start(emulator, geometry), new ClientConfig());
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
            using var emulator = new UdpEmulator(1);
            var client = await Client.OpenAsync(geometry, Driven.Start(emulator, geometry), new ClientConfig());
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
            using var emulator = new UdpEmulator(1);
            await using var client = await Client.OpenAsync(geometry, Driven.Start(emulator, geometry), new ClientConfig());
            await client.CloseAsync();
        }

        [Fact]
        public async Task DisposingTwiceIsSafe()
        {
            using var geometry = SingleDevice();
            using var emulator = new UdpEmulator(1);
            var client = await Client.OpenAsync(geometry, Driven.Start(emulator, geometry), new ClientConfig());
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
                    using var emulator = new UdpEmulator(1);
                    var client = Client.OpenAsync(geometry, Driven.Start(emulator, geometry), new ClientConfig()).GetAwaiter().GetResult();
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
    }
}
