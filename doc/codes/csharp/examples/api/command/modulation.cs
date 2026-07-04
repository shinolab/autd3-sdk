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

        new Modulation(config, data, bank);

        new Modulation(
            config,
            data,
            bank,
            loopBehavior,
            transitionMode
        );
        // ANCHOR_END: api

        // ANCHOR: equivalent
        new WriteModulationBuffer(
            bank,
            0,
            data
        );
        new ConfigModulation(
            bank,
            config,
            (uint)data.Length,
            loopBehavior
        );
        new ChangeModulationBank(
            bank,
            transitionMode
        );
        // ANCHOR_END: equivalent
    }
}
