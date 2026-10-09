using System.Collections.Generic;
using UnityEngine;
using AUTD3;
using static AUTD3.Units;

namespace AUTD3.Samples
{
    // Single focus with a 200 Hz sine AM, driven from a MonoBehaviour.
    //
    // Coordinates are in the Unity frame (metres, left-handed): a device emits along its
    // local -z, so the focus that the dotnet sample writes as new Vector3(0, 0, 150) is
    // new Vector3(0, 0, -0.15f) here.
    //
    // The UdpEmulator and its option runs without hardware; swap it for new TransportOption() on a real device.
    public sealed class FocusSineSample : MonoBehaviour
    {
        private UdpEmulator _emulator;
        private Client _client;
        private Geometry _geometry;

        private async void Start()
        {
            _geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.zero) });
            _emulator = new UdpEmulator(1);
            _client = await Client.OpenAsync(_geometry, _emulator.Option(), new ClientConfig());

            var target = _geometry.Center + new Vector3(0f, 0f, -0.15f);
            var wavelength = Pattern.Wavelength(340 * m / s);
            using var phases = _geometry.PhaseBuffer();
            using var intensities = _geometry.IntensityBuffer();
            Pattern.Focus(_geometry, target, wavelength, phases);

            using var modulation = Modulation.ModulationBuffer();
            Modulation.Sine(200 * Hz, new SineOption(), modulation);

            await _client.SendAsync(new Pattern(phases, intensities));
            await _client.SendAsync(new Modulation(SamplingConfig.Freq4k, modulation));

            Debug.Log($"AUTD3: emitting a 200 Hz AM focus at {target} (Unity frame, metres)");
        }

        private async void OnDestroy()
        {
            if (_client != null)
            {
                await _client.SilentStopAsync();
                await _client.CloseAsync();
                _client.Dispose();
                _client = null;
            }
            _driver?.Dispose();
            _driver = null;
            _geometry?.Dispose();
            _geometry = null;
            _emulator?.Dispose();
            _emulator = null;
        }
    }
}
