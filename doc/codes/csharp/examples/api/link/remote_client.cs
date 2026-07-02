using AUTD3.Link;

namespace AUTD3.DocSamples.ApiLinkRemoteClient;

internal static class Sample
{
    internal static void Run()
    {
        var addr = "127.0.0.1:8080";

        // ANCHOR: api
        RemoteLink.Create(addr);
        // ANCHOR_END: api
    }
}
