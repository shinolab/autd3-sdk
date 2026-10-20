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
        private Client _client;
        private Geometry _geometry;

        private async void Start()
        {
            _geometry = new Geometry(new List<Autd3> { new Autd3(Vector3.zero) });
            _client = Client.Open(_geometry, new TransportOption { Iface = Interface.Simulator(TransportOption.SimulatorPort) });
            await _client.SendAsync(Command.Sequence(new Clear(), new Synchronize()));

            var target = _geometry.Center + new Vector3(0f, 0f, -0.15f);
            var wavelength = Pattern.Wavelength(340 * m / s);
            using var phases = _geometry.PhaseBuffer();
            using var intensities = _geometry.IntensityBuffer();
            Pattern.Focus(_geometry, target, wavelength, phases);

            using var modulation = Modulation.ModulationBuffer();
            var samplingConfig = SamplingConfig.Freq4k;
            Modulation.Sine(200 * Hz, new SineOption(), samplingConfig, modulation);

            await _client.SendAsync(new Pattern(phases, intensities));
            await _client.SendAsync(new Modulation(samplingConfig, modulation));

            Debug.Log($"AUTD3: emitting a 200 Hz AM focus at {target} (Unity frame, metres)");
        }

        private async void OnDestroy()
        {
            if (_client != null)
            {
                await _client.CloseAsync();
                _client.Dispose();
                _client = null;
            }
            _geometry?.Dispose();
            _geometry = null;
        }
    }
}
