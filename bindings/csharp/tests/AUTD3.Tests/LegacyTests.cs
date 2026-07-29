using System.Numerics;
using System.Threading.Tasks;
using AUTD3.Legacy;
using Xunit;
using static AUTD3.Units;

namespace AUTD3.Tests
{
    public class LegacyTests
    {
        private static Geometry SingleDevice() => new Geometry(new[] { new Autd3(Vector3.Zero) });

        [Fact]
        public async Task OpenReportsLegacyFirmwareVersion()
        {
            using var geometry = SingleDevice();
            using var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());

            Assert.Equal(1, client.NumDevices);
            var versions = await client.ReadFirmwareVersionAsync();
            Assert.Contains("legacy-v12.1.0", Assert.Single(versions));
            await client.CloseAsync();
        }

        [Fact]
        public async Task CurrentCommandTypesDriveTheLegacyClient()
        {
            using var geometry = SingleDevice();
            using var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());

            using var patterns = geometry.PatternBuffer();
            Pattern.Focus(geometry, geometry.Center + new Vector3(0f, 0f, 150f), Pattern.Wavelength(340 * m / s),
                new FocusOption(), patterns);
            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption(), modulation);

            using var builder = client.DatagramBuilder();
            builder
                .Push(new SetSilencer())
                .Push(new Pattern(patterns))
                .Push(new Modulation(SamplingConfig.Freq4k, modulation));
            using var frames = builder.Build();
            Assert.True(frames.Length > 0);
            foreach (var frame in frames)
            {
                await client.SendCheckedAsync(frame);
            }

            var states = await client.ReadFpgaStateAsync();
            Assert.Single(states);

            await client.StopAsync();
            await client.CloseAsync();
        }

        [Fact]
        public async Task OpenWithCheckerReportsStatus()
        {
            using var geometry = SingleDevice();
            var (client, checker) = await LegacyClient.OpenWithCheckerAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());
            using var c = client;
            using var k = checker;

            Assert.Equal(1, client.NumDevices);
            var status = await checker.CheckAsync();
            Assert.Equal(DeviceState.Op, Assert.Single(status.Devices));
            Assert.True(status.AllOp);
            Assert.False(status.AnyLost);
            Assert.Equal(0UL, status.Recoveries);

            await client.CloseAsync();
        }

        [Fact]
        public async Task PushEachAssignsPerDevice()
        {
            using var geometry = new Geometry(new[]
            {
                new Autd3(Vector3.Zero),
                new Autd3(new Vector3(Autd3.DeviceWidth, 0f, 0f)),
            });
            using var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());

            using var patterns = geometry.PatternBuffer();
            Pattern.Focus(geometry, geometry.Center + new Vector3(0f, 0f, 150f), Pattern.Wavelength(340 * m / s),
                new FocusOption(), patterns);

            using var builder = client.DatagramBuilder();
            builder.PushEach(device => device.Idx == 0 ? new Pattern(patterns) : (ICommand?)null);
            using var frames = builder.Build();
            Assert.Equal(1, frames.Length);
            foreach (var frame in frames)
            {
                await client.SendCheckedAsync(frame);
            }

            await client.CloseAsync();
        }

        [Fact]
        public async Task PushEachRejectsUnsupportedCommandsAtBuildTime()
        {
            using var geometry = SingleDevice();
            using var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());

            using var builder = client.DatagramBuilder();
            builder.PushEach(_ => new ChangePatternBank(PatternBank.B1));
            var e = Assert.Throws<Autd3Exception>(() => builder.Build());
            Assert.Contains("ChangePatternBank", e.Message);

            await client.CloseAsync();
        }

        [Fact]
        public async Task UnsupportedCommandsAreRejectedAtBuildTime()
        {
            using var geometry = SingleDevice();
            using var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());

            using var builder = client.DatagramBuilder();
            builder.Push(new ChangePatternBank(PatternBank.B1));
            var e = Assert.Throws<Autd3Exception>(() => builder.Build());
            Assert.Contains("ChangePatternBank", e.Message);

            await client.CloseAsync();
        }

        [Fact]
        public async Task LaterStagesAModulationBankThenLegacyChangePatternBankSwitchesIt()
        {
            using var geometry = SingleDevice();
            using var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());

            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption(), modulation);

            using var builder = client.DatagramBuilder();
            builder
                .Push(new Modulation(ModulationBank.B1, SamplingConfig.Freq4k, modulation,
                    transitionMode: TransitionMode.Later))
                .Push(new ChangeModulationBank(ModulationBank.B1));
            using var frames = builder.Build();
            Assert.True(frames.Length > 0);
            foreach (var frame in frames)
            {
                await client.SendCheckedAsync(frame);
            }

            await client.CloseAsync();
        }

        [Fact]
        public async Task ALegacyBankChangeRefusesToNotTransition()
        {
            using var geometry = SingleDevice();
            using var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());

            using var builder = client.DatagramBuilder();
            builder.Push(new ChangeModulationBank(ModulationBank.B1, TransitionMode.Later));
            var e = Assert.Throws<Autd3Exception>(() => builder.Build());
            Assert.Contains("Later", e.Message);

            await client.CloseAsync();
        }

        [Fact]
        public async Task LegacyChangePatternBankSwitchesThePatternBank()
        {
            using var geometry = SingleDevice();
            using var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());

            var commands = new ILegacyCommand[]
            {
                LegacyChangePatternBank.Pattern(PatternBank.B1),
                LegacyChangePatternBank.FociStm(PatternBank.B0),
                LegacyChangePatternBank.FociStm(PatternBank.B1, TransitionMode.SyncIdx),
                LegacyChangePatternBank.PatternStm(PatternBank.B0),
                LegacyChangePatternBank.PatternStm(PatternBank.B1, TransitionMode.Ext),
            };
            foreach (var command in commands)
            {
                using var each = client.DatagramBuilder();
                each.Push(command);
                using var built = each.Build();
                Assert.Equal(1, built.Length);
            }

            using var builder = client.DatagramBuilder();
            builder.Push(LegacyChangePatternBank.Pattern(PatternBank.B0));
            using var frames = builder.Build();
            await client.SendCheckedAsync(frames[0]);

            await client.CloseAsync();
        }

        [Fact]
        public async Task SendAsyncReturnsOneResponseBytePerDevice()
        {
            using var geometry = new Geometry(new[]
            {
                new Autd3(Vector3.Zero),
                new Autd3(new Vector3(Autd3.DeviceWidth, 0f, 0f)),
            });
            using var client = await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig());

            using var builder = client.DatagramBuilder();
            builder.Push(new ForceFan(true));
            using var frames = builder.Build();
            Assert.Equal(1, frames.Length);

            var response = await client.SendAsync(frames[0]);
            Assert.Equal(client.NumDevices, response.Length);

            await client.CloseAsync();
        }

        [Fact]
        public async Task ZeroTimeoutCyclesIsRejected()
        {
            using var geometry = SingleDevice();
            await Assert.ThrowsAsync<Autd3Exception>(async () =>
                await LegacyClient.OpenAsync(geometry, new AUTD3.Link.Nop(), new LegacyClientConfig(timeoutCycles: 0)));
        }
    }
}
