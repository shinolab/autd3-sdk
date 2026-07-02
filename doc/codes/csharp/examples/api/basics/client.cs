using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

namespace AUTD3.DocSamples.ApiBasicsClient;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });
        var client = await Client.OpenAsync(geometry, NopLink.Create(), new ClientConfig());

        var frames = client.DatagramBuilder().Build();
        var frame = frames[0];

        // ANCHOR: api
        var numDevices = client.NumDevices;

        var firmware = await client.ReadFirmwareVersionAsync();
        var fpgaState = await client.ReadFpgaStateAsync();
        var errorDetail = await client.ReadErrorDetailAsync();

        var datagramBuilder = client.DatagramBuilder();
        var token = await client.SendAsync(frame);
        await token.AwaitAsync();
        await client.SendCheckedAsync(frame);

        await client.StopAsync();
        await client.CloseAsync();
        // ANCHOR_END: api

        _ = (numDevices, firmware, fpgaState, errorDetail, datagramBuilder);
    }
}
