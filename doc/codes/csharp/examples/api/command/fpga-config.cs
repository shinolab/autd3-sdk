using AUTD3;

namespace DocSamples.ApiCommandFpgaConfig;

internal static class Sample
{
    internal static void Run()
    {
        var syncPhaseLock = true;
        // ANCHOR: api
        var config = new FpgaConfig
        {
            SyncPhaseLock = syncPhaseLock,
        };
        new SetFpgaConfig(config);
        // ANCHOR_END: api
    }
}
