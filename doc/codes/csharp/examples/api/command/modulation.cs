using System;
using AUTD3;

namespace DocSamples.ApiCommandModulation;

internal static class Sample
{
    internal static void Run()
    {
        var bank = ModulationBank.B0;
        var config = SamplingConfig.Freq4k;
        var loopBehavior = LoopBehavior.Infinite;
        var transitionMode = TransitionMode.Immediate;

        var data = Modulation.ModulationBuffer();

        // ANCHOR: api
        new Modulation(config, data);

        new Modulation(bank, config, data);

        new Modulation(
            bank: bank,
            config: config,
            data: data,
            loopBehavior: loopBehavior,
            transitionMode: transitionMode
        );
        // ANCHOR_END: api

        // ANCHOR: equivalent
        var samples = data.ToArray();
        for (var offset = 0; offset < samples.Length; offset += WriteModulationBuffer.MaxLen)
        {
            using var chunk = ModulationBuffer.FromBytes(samples[offset..Math.Min(offset + WriteModulationBuffer.MaxLen, samples.Length)]);
            new WriteModulationBuffer(
                bank: bank,
                offset: (uint)offset,
                data: chunk
            );
        }
        new ConfigModulation(
            bank: bank,
            config: config,
            size: (uint)data.Length,
            loopBehavior: loopBehavior
        );
        new ActivateModulationBank(
            bank: bank,
            transitionMode: transitionMode
        );
        // ANCHOR_END: equivalent
    }
}
