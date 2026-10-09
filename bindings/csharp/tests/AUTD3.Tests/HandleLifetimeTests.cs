using System;
using AUTD3;
using Xunit;

namespace AUTD3.Tests
{
    public class HandleLifetimeTests
    {
        [Fact]
        public void DisposingTwiceIsHarmless()
        {
            var geometry = Fixture.SingleDevice();
            geometry.Dispose();
            geometry.Dispose();
        }

        [Fact]
        public void UsingADisposedGeometryThrowsInsteadOfTouchingFreedMemory()
        {
            var geometry = Fixture.SingleDevice();
            geometry.Dispose();
            Assert.Throws<ObjectDisposedException>(() => geometry.NumDevices);
        }

        [Fact]
        public void UsingADisposedPatternBufferThrowsInsteadOfTouchingFreedMemory()
        {
            using var geometry = Fixture.SingleDevice();
            var phases = geometry.PhaseBuffer();
            phases.Dispose();
            Assert.Throws<ObjectDisposedException>(() => phases.NumDevices);
            var intensities = geometry.IntensityBuffer();
            intensities.Dispose();
            Assert.Throws<ObjectDisposedException>(() => intensities.NumDevices);
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
            using var geometry = Fixture.SingleDevice();
            var frames = Frames.Encode(geometry, new Nop());
            frames.Dispose();
            Assert.Throws<ObjectDisposedException>(() => frames.Length);
        }

        [Fact]
        public void ADeviceViewOutlivingItsGeometryThrowsInsteadOfTouchingFreedMemory()
        {
            var geometry = Fixture.SingleDevice();
            var device = geometry[0];
            Assert.Equal(0, device.Idx);
            geometry.Dispose();
            Assert.Throws<ObjectDisposedException>(() => device.Idx);
        }
    }
}
