using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiBasicsDriverThread;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
        using var emulator = new UdpEmulator(geometry.NumDevices);

        var (driver, connector) = Driver.Open(emulator.Option(), geometry.NumDevices);
        using var _driver = driver;
        var thread = new Thread(driver.Run) { IsBackground = true };
        thread.Start();

        var client = await Client.OpenAsync(geometry, connector, new ClientConfig());
        await client.CloseAsync();
        client.Dispose();

        thread.Join();
    }
}
