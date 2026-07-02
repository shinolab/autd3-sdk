using AUTD3;
using static AUTD3.Units;

namespace AUTD3.DocSamples.ApiCommandLowlevelModBank;

internal static class Sample
{
    internal static void Run()
    {
        var bank = ModulationBank.B0;
        var offset = 0u;
        var data = Modulation.ModulationBuffer();
        Modulation.Sine(150 * Hz, new SineOption(), data);
        // ANCHOR: write
        new WriteModulationBuffer(bank, offset, data);
        // ANCHOR_END: write
        var config = SamplingConfig.Freq4k;
        var size = (uint)data.Length;
        var loopBehavior = LoopBehavior.Infinite;
        // ANCHOR: config
        new ConfigModulation(bank, config, size, loopBehavior);
        // ANCHOR_END: config
        var transitionMode = TransitionMode.Immediate;
        // ANCHOR: change
        new ChangeModulationBank(bank, transitionMode);
        // ANCHOR_END: change
    }
}
