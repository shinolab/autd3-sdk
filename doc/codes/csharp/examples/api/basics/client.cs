using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiBasicsClient;

internal static class Sample
{
    internal static async Task Run()
    {
        var layout = new Geometry(new[] { new Autd3(Vector3.Zero) });
        using var emulator = new UdpEmulator(layout.NumDevices);
        var (driver, connector) = Driver.Open(emulator.Option(), layout.NumDevices);
        new Thread(driver.Run) { IsBackground = true }.Start();
        var client = await Client.OpenAsync(layout, connector, new ClientConfig());

        var frames = client.DatagramBuilder().Build();
        var frame = frames[0];

        // ANCHOR: api
        var numDevices = client.NumDevices;
        var geometry = client.Geometry;

        var firmware = await client.ReadFirmwareVersionAsync();
        var fpgaState = await client.ReadFpgaStateAsync();
        var errorDetail = await client.ReadErrorDetailAsync();

        var datagramBuilder = client.DatagramBuilder();
        var resp = await await client.SendAsync(frame);
        await client.SendCheckedAsync(frame);

        await client.StopAsync();
        await client.CloseAsync();
        // ANCHOR_END: api

        _ = (numDevices, geometry, firmware, fpgaState, errorDetail, datagramBuilder);

        var scopedLayout = new Geometry(new[] { new Autd3(Vector3.Zero) });

        // ANCHOR: context_manager
        using var scopedEmulator = new UdpEmulator(scopedLayout.NumDevices);
        var (scopedDriver, scopedConnector) = Driver.Open(scopedEmulator.Option(), scopedLayout.NumDevices);
        new Thread(scopedDriver.Run) { IsBackground = true }.Start();
        await using (var scoped = await Client.OpenAsync(scopedLayout, scopedConnector, new ClientConfig()))
        {
            await scoped.SendCheckedAsync(frame);
        }
        // ANCHOR_END: context_manager
    }
}
