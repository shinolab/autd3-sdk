using System;
using System.Numerics;
using AUTD3;
using Xunit;

namespace AUTD3.Tests
{
    public class HandleLifetimeTests
    {
        private static Geometry SingleDevice() => new Geometry(new[] { new Autd3(Vector3.Zero) });

        [Fact]
        public void DisposingTwiceIsHarmless()
        {
            var geometry = SingleDevice();
            geometry.Dispose();
            geometry.Dispose();
        }

        [Fact]
        public void UsingADisposedGeometryThrowsInsteadOfTouchingFreedMemory()
        {
            var geometry = SingleDevice();
            geometry.Dispose();
            Assert.Throws<ObjectDisposedException>(() => geometry.NumDevices);
        }

        [Fact]
        public void UsingADisposedPatternBufferThrowsInsteadOfTouchingFreedMemory()
        {
            using var geometry = SingleDevice();
            var buffer = geometry.PatternBuffer();
            buffer.Dispose();
            Assert.Throws<ObjectDisposedException>(() => buffer.NumDevices);
        }

        [Fact]
        public void UsingADisposedModulationBufferThrowsInsteadOfTouchingFreedMemory()
        {
            var buffer = new ModulationBuffer(4);
            buffer.Dispose();
            Assert.Throws<ObjectDisposedException>(() => buffer.Length);
        }

        [Fact]
        public void UsingADisposedFramesThrowsInsteadOfTouchingFreedMemory()
        {
            using var geometry = SingleDevice();
            using var builder = new DatagramBuilder(geometry);
            var frames = builder.Push(new Nop()).Build();
            frames.Dispose();
            Assert.Throws<ObjectDisposedException>(() => frames.Length);
        }

        [Fact]
        public void ADeviceViewOutlivingItsGeometryThrowsInsteadOfTouchingFreedMemory()
        {
            var geometry = SingleDevice();
            var device = geometry[0];
            Assert.Equal(0, device.Idx);
            geometry.Dispose();
            Assert.Throws<ObjectDisposedException>(() => device.Idx);
        }
    }
}
