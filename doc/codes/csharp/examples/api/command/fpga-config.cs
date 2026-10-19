using AUTD3;

namespace DocSamples.ApiCommandFpgaConfig;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        var config = new FpgaConfig
        {
            SyncPhaseLock = true,
        };
        new SetFpgaConfig(config);
        // ANCHOR_END: api
    }
}
