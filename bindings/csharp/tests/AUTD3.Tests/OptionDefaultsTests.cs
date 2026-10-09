using System;
using AUTD3.Holo;
using Xunit;

namespace AUTD3.Tests
{
    public class OptionDefaultsTests
    {
        [Fact]
        public void ClientConfigMatchesTheRustDefault()
        {
            var config = new ClientConfig();
            Assert.Equal(TimeSpan.FromMilliseconds(10), config.AckTimeout);
            Assert.Equal(7u, config.MaxInflight);
            Assert.Equal(8u, config.MaxResyncRounds);
            Assert.False(config.RequireSupportedFirmware);
        }

        [Fact]
        public void CpuConfigMatchesTheRustDefault()
        {
            var config = new CpuConfig();
            Assert.Equal(TimeSpan.FromMilliseconds(10), config.SysTimeTransitionMargin);
            Assert.Equal(1_000_000u, config.FpgaWaitUpdateMaxPolls);
            Assert.Equal(2_000_000_000u, config.FpgaFlashMaxPolls);
            Assert.Equal(TimeSpan.FromTicks(2500), config.SyncGuard);
            Assert.Equal(TimeSpan.FromMilliseconds(100), config.UpdateActivateDelay);
            Assert.Equal(TimeSpan.FromMilliseconds(500), config.FailsafeTimeout);
            Assert.Equal(new PtpConfig(), config.Ptp);

            var ptp = new PtpConfig();
            Assert.Equal(TimeSpan.FromMilliseconds(8), ptp.SyncInterval);
            Assert.Equal(TimeSpan.FromMilliseconds(3), ptp.TxTimestampTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(6), ptp.DelayRespTimeout);
            Assert.Equal(TimeSpan.FromSeconds(1), ptp.Holdover);
            Assert.Equal((ushort)64, ptp.LockSamples);
            Assert.Equal(TimeSpan.FromTicks(100), ptp.StepThreshold);
            Assert.Equal(TimeSpan.FromTicks(1), ptp.LockThreshold);
            Assert.Equal(50u, ptp.KpMilli);
            Assert.Equal(1u, ptp.KiMilli);
            Assert.Equal(500_000u, ptp.MaxFreqPpb);
            Assert.Equal((ushort)32, ptp.DelayReqSyncs);
            Assert.Equal((byte)5, ptp.PathDelayFilterShift);
            Assert.Equal((ushort)48, ptp.PauseQuanta);
            Assert.Equal((ushort)48, default(PtpConfig).PauseQuanta);
            Assert.Null(new PtpConfig { PauseQuanta = null }.PauseQuanta);
            Assert.Equal((ushort)64, ptp.PauseHoldSyncs);
            Assert.Equal(TimeSpan.FromMilliseconds(8), ptp.PauseRetry);
        }

        [Fact]
        public void CpuConfigInitializerKeepsTheOtherDefaults()
        {
            var config = new CpuConfig
            {
                SysTimeTransitionMargin = TimeSpan.Zero,
                Ptp = new PtpConfig { LockSamples = 128 },
            };
            Assert.Equal(TimeSpan.Zero, config.SysTimeTransitionMargin);
            Assert.Equal(new CpuConfig().FpgaWaitUpdateMaxPolls, config.FpgaWaitUpdateMaxPolls);
            Assert.Equal(new CpuConfig().SyncGuard, config.SyncGuard);
            Assert.Equal(TimeSpan.FromMilliseconds(500), config.FailsafeTimeout);
            Assert.Null(new CpuConfig { FailsafeTimeout = null }.FailsafeTimeout);
            Assert.Equal(TimeSpan.FromMilliseconds(500), default(CpuConfig).FailsafeTimeout);
            Assert.Null(config.PtpUnlockFailsafeTimeout);
            Assert.Null(default(CpuConfig).PtpUnlockFailsafeTimeout);
            Assert.Equal(TimeSpan.FromSeconds(2), new CpuConfig { PtpUnlockFailsafeTimeout = TimeSpan.FromSeconds(2) }.PtpUnlockFailsafeTimeout);
            Assert.Equal(FpgaBusWait.Cycles3, config.FpgaBusWait);
            Assert.Equal(FpgaBusWait.Cycles3, default(CpuConfig).FpgaBusWait);
            Assert.Equal(FpgaBusWait.Cycles2, new CpuConfig { FpgaBusWait = FpgaBusWait.Cycles2 }.FpgaBusWait);
            Assert.Equal((ushort)128, config.Ptp.LockSamples);
            Assert.Equal(new PtpConfig().SyncInterval, config.Ptp.SyncInterval);
            Assert.Equal(new PtpConfig().KpMilli, config.Ptp.KpMilli);
        }

        [Fact]
        public void InitializerKeepsTheOtherDefaults()
        {
            var config = new ClientConfig { MaxInflight = 4 };
            Assert.Equal(4u, config.MaxInflight);
            Assert.Equal(TimeSpan.FromMilliseconds(10), config.AckTimeout);

            var gs = new GsOption { Repeat = 5 };
            Assert.Equal(5u, gs.Repeat);
            Assert.Equal(new GsOption().Constraint, gs.Constraint);
            Assert.True(gs.Parallel);
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
            Assert.Equal(PhaseDepth.Bits8, pattern.PhaseDepth);
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

        [Theory]
        [InlineData(typeof(ClientConfig))]
        [InlineData(typeof(TransportOption))]
        [InlineData(typeof(CpuConfig))]
        [InlineData(typeof(PtpConfig))]
        [InlineData(typeof(FociStmOption))]
        [InlineData(typeof(PatternStmOption))]
        [InlineData(typeof(SineOption))]
        [InlineData(typeof(SquareOption))]
        [InlineData(typeof(FourierOption))]
        [InlineData(typeof(NaiveOption))]
        [InlineData(typeof(GsOption))]
        [InlineData(typeof(GspatOption))]
        [InlineData(typeof(GreedyOption))]
        public void TheDefaultValueEqualsTheConstructedOne(Type type)
        {
            var constructed = Activator.CreateInstance(type);
            var zeroed = System.Runtime.CompilerServices.RuntimeHelpers.GetUninitializedObject(type);
            foreach (var property in type.GetProperties())
            {
                Assert.Equal(property.GetValue(constructed), property.GetValue(zeroed));
            }
        }

        [Fact]
        public void TheDefaultValueCarriesTheRustDefaults()
        {
            Assert.Equal(7u, default(ClientConfig).MaxInflight);
            Assert.Equal(TimeSpan.FromMilliseconds(100), default(TransportOption).LostTimeout);
            Assert.Equal(new PtpConfig().SyncInterval, default(CpuConfig).Ptp.SyncInterval);
            Assert.NotEqual(TimeSpan.Zero, default(PtpConfig).SyncInterval);
            Assert.Equal((byte)0xFF, default(SineOption).Amplitude);
            Assert.Equal(100u, default(GsOption).Repeat);
        }
    }
}
