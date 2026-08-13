using System;
using AUTD3.Link;

namespace DocSamples.ApiLinkRemoteClient;

internal static class Sample
{
    internal static void Run()
    {
        var addr = "127.0.0.1:8080";
        var timeout = TimeSpan.FromSeconds(1);

        // ANCHOR: api
        new RemoteLinkOption(addr, timeout);
        // ANCHOR_END: api
    }

    internal static void Discover()
    {
        // ANCHOR: discover
        RemoteLinkOption.Discover();
        // ANCHOR_END: discover
    }

    internal static void DiscoverWithOption()
    {
        var timeout = TimeSpan.FromSeconds(1);
        // ANCHOR: discover_option
        RemoteLinkOption.Discover(timeout, "autd3-0a1b2c3d");
        // ANCHOR_END: discover_option
    }
}
