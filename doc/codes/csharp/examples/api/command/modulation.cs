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
        new WriteModulationBuffer(
            bank: bank,
            offset: 0,
            data: data
        );
        new ConfigModulation(
            bank: bank,
            config: config,
            size: (uint)data.Length,
            loopBehavior: loopBehavior
        );
        new ChangeModulationBank(
            bank: bank,
            transitionMode: transitionMode
        );
        // ANCHOR_END: equivalent
    }
}
