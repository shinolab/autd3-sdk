using AUTD3.Holo;
using AUTD3.Legacy;
using Xunit;

namespace AUTD3.Tests
{
    public class OptionDefaultsTests
    {
        [Fact]
        public void ClientConfigMatchesTheRustDefault()
        {
            var config = new ClientConfig();
            Assert.False(config.LowLatency);
            Assert.Equal(10u, config.TimeoutCycles);
            Assert.Equal((uint)Client.MaxInflight, config.MaxInflight);
            Assert.Equal(8u, config.MaxResyncRounds);
            Assert.Equal(2u, config.ResetResendCycles);
            Assert.Equal(RtPriority.Default, config.RtPriority);
            Assert.Equal(RtSchedulePolicy.Fifo, config.RtPolicy);
            Assert.Null(config.RtAffinity);
            Assert.True(config.ValidateState);
            Assert.False(config.RequireSupportedFirmware);
        }

        [Fact]
        public void InitializerKeepsTheOtherDefaults()
        {
            var config = new ClientConfig { MaxInflight = 4, RtPriority = null };
            Assert.Equal(4u, config.MaxInflight);
            Assert.Null(config.RtPriority);
            Assert.Equal(10u, config.TimeoutCycles);
            Assert.True(config.ValidateState);

            var gs = new GsOption { Repeat = 5 };
            Assert.Equal(5u, gs.Repeat);
            Assert.Equal(new GsOption().Constraint, gs.Constraint);
            Assert.True(gs.Parallel);
        }

        [Fact]
        public void LegacyClientConfigMatchesTheRustDefault()
        {
            var config = new LegacyClientConfig();
            Assert.Equal(2000u, config.TimeoutCycles);
            Assert.Equal(RtPriority.Default, config.RtPriority);
            Assert.Equal(RtSchedulePolicy.Fifo, config.RtPolicy);
            Assert.Null(config.RtAffinity);
        }

        [Fact]
        public void StmOptionsMatchTheRustDefault()
        {
            var foci = new FociStmOption();
            Assert.Equal(PatternBank.B0, foci.Bank);
            Assert.Equal(340f, foci.SoundSpeed.MS);
            Assert.Equal(LoopBehavior.Infinite, foci.LoopBehavior);
            Assert.Equal(TransitionMode.Immediate, foci.TransitionMode);

            var pattern = new PatternStmOption();
            Assert.Equal(PatternBank.B0, pattern.Bank);
            Assert.Equal(PatternStmMode.PhaseIntensityFull, pattern.Mode);
            Assert.Equal(LoopBehavior.Infinite, pattern.LoopBehavior);
            Assert.Equal(TransitionMode.Immediate, pattern.TransitionMode);
        }

        [Fact]
        public void ModulationOptionsMatchTheRustDefault()
        {
            var sine = new SineOption();
            Assert.Equal(0xFF, sine.Amplitude);
            Assert.Equal(0x80, sine.Offset);
            Assert.Equal(0f, sine.Phase.Rad);
            Assert.False(sine.Clamp);
            Assert.Equal(SamplingConfig.Freq4k, sine.SamplingConfig);

            var square = new SquareOption();
            Assert.Equal(0x00, square.Low);
            Assert.Equal(0xFF, square.High);
            Assert.Equal(0.5f, square.Duty);
            Assert.Equal(SamplingConfig.Freq4k, square.SamplingConfig);

            var fourier = new FourierOption();
            Assert.Null(fourier.ScaleFactor);
            Assert.False(fourier.Clamp);
            Assert.Equal(0x00, fourier.Offset);
        }

        [Fact]
        public void HoloOptionsMatchTheRustDefault()
        {
            var clamp = IntensityConstraint.Clamp(Intensity.Min, Intensity.Max);

            var naive = new NaiveOption();
            Assert.Equal(clamp, naive.Constraint);
            Assert.Equal(Directivity.Sphere, naive.Directivity);
            Assert.Equal(TransducerMask.AllEnabled, naive.Mask);
            Assert.True(naive.Parallel);

            var gs = new GsOption();
            Assert.Equal(100u, gs.Repeat);
            Assert.Equal(clamp, gs.Constraint);
            Assert.Equal(Directivity.Sphere, gs.Directivity);
            Assert.Equal(TransducerMask.AllEnabled, gs.Mask);
            Assert.True(gs.Parallel);

            var gspat = new GspatOption();
            Assert.Equal(100u, gspat.Repeat);
            Assert.Equal(clamp, gspat.Constraint);
            Assert.Equal(Directivity.Sphere, gspat.Directivity);
            Assert.Equal(TransducerMask.AllEnabled, gspat.Mask);
            Assert.True(gspat.Parallel);

            var greedy = new GreedyOption();
            Assert.Equal(16, greedy.PhaseQuantizationLevels);
            Assert.Equal(IntensityConstraint.Uniform(Intensity.Max), greedy.Constraint);
            Assert.Equal(Directivity.Sphere, greedy.Directivity);
            Assert.Equal(TransducerMask.AllEnabled, greedy.Mask);
        }
    }
}
