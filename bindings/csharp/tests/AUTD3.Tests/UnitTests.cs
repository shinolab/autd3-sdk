using System;
using AUTD3;
using Xunit;
using static AUTD3.Units;
using AUTD3.Holo;

namespace AUTD3.Tests
{
    public class UnitTests
    {
        [Fact]
        public void FreqFromInt()
        {
            Assert.Equal(200f, (200 * Hz).Hz);
            Assert.Equal(2000f, (2 * kHz).Hz);
        }

        [Fact]
        public void FreqFromFloat()
        {
            Assert.Equal(200.5f, (200.5f * Hz).Hz);
        }

        [Fact]
        public void FreqNearest()
        {
            Assert.Equal(200f, Nearest(200 * Hz).Value.Hz);
        }

        [Fact]
        public void VelocityNoParentheses()
        {
            Assert.Equal(340000f, (340 * m / s).MmPerS);
        }

        [Fact]
        public void LengthConversions()
        {
            Assert.Equal(5f, (5 * mm).Mm);
            Assert.Equal(5000f, (5 * m).Mm);
        }

        [Fact]
        public void AngleConversions()
        {
            Assert.Equal((float)(Math.PI / 2.0), (90 * deg).Radian, 4);
            Assert.Equal(1f, (1 * rad).Radian);
        }

        [Fact]
        public void IntAndFloatFreqBothDriveSine()
        {
            using var intMod = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption(), intMod);
            Assert.True(intMod.Length > 0);

            using var floatMod = Modulation.ModulationBuffer();
            Modulation.Sine(200.0f * Hz, new SineOption(), floatMod);
            Assert.True(floatMod.Length > 0);

            using var nearestMod = Modulation.ModulationBuffer();
            Modulation.Sine(Nearest(200 * Hz), new SineOption(), nearestMod);
            Assert.True(nearestMod.Length > 0);
        }

        [Fact]
        public void SamplingConfigFromFreqRoundTrips()
        {
            var config = new SamplingConfig(4000 * Hz);
            Assert.InRange(config.Freq().Hz, 3999f, 4001f);
        }

        [Fact]
        public void AmplitudeSplRoundTrips()
        {
            var amp = Amplitude.FromSpl(121.5f);
            Assert.Equal(121.5f, amp.Spl, 2);
        }
    }
}
