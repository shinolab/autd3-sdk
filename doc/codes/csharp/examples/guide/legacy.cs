using System;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Legacy;
using AUTD3.Link;
using static AUTD3.Units;

namespace DocSamples.GuideLegacy;

internal static class Sample
{
    internal static async Task Run()
    {
        using var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        // ANCHOR: open
        using var client = await LegacyClient.OpenAsync(
            geometry,
            new EchocatLinkOption(),
            new LegacyClientConfig()
        );
        // ANCHOR_END: open

        // ANCHOR: version
        var versions = await client.ReadFirmwareVersionAsync();
        for (var i = 0; i < versions.Count; i++)
        {
            Console.WriteLine($"device[{i}] firmware version: {versions[i]}");
        }
        // ANCHOR_END: version

        var target = geometry.Center + new Vector3(0.0f, 0.0f, 150.0f);
        using var emissions = geometry.PatternBuffer();
        Pattern.Focus(geometry, target, Pattern.Wavelength(340.0f * m / s), new FocusOption(), emissions);
        using var modulation = Modulation.ModulationBuffer();
        Modulation.Sine(200 * Hz, new SineOption(), modulation);

        // ANCHOR: send
        using var builder = client.DatagramBuilder();
        builder
            .Push(new SetSilencer())
            .Push(new Pattern(emissions))
            .Push(new Modulation(SamplingConfig.Freq4k, modulation));
        using var frames = builder.Build();
        foreach (var frame in frames)
        {
            await client.SendCheckedAsync(frame);
        }
        // ANCHOR_END: send

        // ANCHOR: change_bank
        using (var bankBuilder = client.DatagramBuilder())
        {
            bankBuilder.Push(new Pattern(PatternBank.B1, emissions));
            using var bankFrames = bankBuilder.Build();
            foreach (var frame in bankFrames)
            {
                await client.SendCheckedAsync(frame);
            }
        }

        using (var bankBuilder = client.DatagramBuilder())
        {
            bankBuilder.Push(LegacyChangePatternBank.Pattern(PatternBank.B0));
            using var bankFrames = bankBuilder.Build();
            foreach (var frame in bankFrames)
            {
                await client.SendCheckedAsync(frame);
            }
        }
        // ANCHOR_END: change_bank

        // ANCHOR: later
        using (var laterBuilder = client.DatagramBuilder())
        {
            laterBuilder.Push(new Modulation(ModulationBank.B1, SamplingConfig.Freq4k, modulation,
                transitionMode: TransitionMode.Later));
            using var laterFrames = laterBuilder.Build();
            foreach (var frame in laterFrames)
            {
                await client.SendCheckedAsync(frame);
            }
        }

        using (var laterBuilder = client.DatagramBuilder())
        {
            laterBuilder.Push(new ChangeModulationBank(ModulationBank.B1, TransitionMode.Immediate));
            using var laterFrames = laterBuilder.Build();
            foreach (var frame in laterFrames)
            {
                await client.SendCheckedAsync(frame);
            }
        }
        // ANCHOR_END: later

        // ANCHOR: close
        await client.StopAsync();
        await client.CloseAsync();
        // ANCHOR_END: close
    }
}
