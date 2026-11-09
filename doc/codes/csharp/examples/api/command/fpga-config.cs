using AUTD3;

namespace DocSamples.ApiCommandFpgaConfig;

internal static class Sample
{
    internal static void Run()
    {
        var syncPhaseLock = true;
        ushort syncKp = 64;
        ushort syncKi = 1024;
        ushort syncKf = 4096;
        // ANCHOR: api
        var config = new FpgaConfig
        {
            SyncPhaseLock = syncPhaseLock,
            SyncKp = syncKp,
            SyncKi = syncKi,
            SyncKf = syncKf,
        };
        new SetFpgaConfig(config);
        // ANCHOR_END: api
    }
}
