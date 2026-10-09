import asyncio
import enum
import signal

import autd3
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3.commands import Pattern, SetSilencer
from autd3.geometry import Autd3, Device, Geometry, offset
from autd3.units import m, mm, s
from autd3_pattern_holo import Pa


class Side(enum.Enum):
    LEFT = enum.auto()
    RIGHT = enum.auto()


async def main() -> None:
    _log_guard = autd3.init_tracing()

    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])

    async with await autd3.Client.open(
        geometry,
        autd3.TransportOption(),
        autd3.ClientConfig(),
    ) as client:
        print("devices:", client.num_devices())

        wavelength = pattern.wavelength(340 * m / s)
        center = geometry.center()

        def side_of(device: Device, tr: int) -> Side:
            return Side.LEFT if device.position(tr)[0] < center[0] else Side.RIGHT

        groups = pattern.TransducerGroups(geometry, side_of)

        left_foci = [
            holo.AmplitudeTarget(center + offset(-50.0 * mm, 0.0 * mm, 150.0 * mm), 2.5e3 * Pa),
            holo.AmplitudeTarget(center + offset(-20.0 * mm, 0.0 * mm, 150.0 * mm), 2.5e3 * Pa),
        ]
        right_target = center + offset(40.0 * mm, 0.0 * mm, 150.0 * mm)

        def compute(
            side: Side,
            mask: pattern.TransducerMask,
            phases: pattern.PhaseBuffer,
            intensities: pattern.IntensityBuffer,
        ) -> None:
            if side is Side.LEFT:
                holo.gspat(geometry, left_foci, wavelength, holo.GspatOption(mask=mask), phases, intensities)
            else:
                pattern.focus(geometry, right_target, wavelength, phases)

        phases = geometry.phase_buffer()
        intensities = geometry.intensity_buffer()
        pattern.group_compute(geometry, groups, compute, phases, intensities)

        await client.send(SetSilencer())
        await client.send(Pattern(phases, intensities))

        print("left half -> two GSPAT foci, right half -> single focus — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        await stop.wait()

        await client.silent_stop()


if __name__ == "__main__":
    asyncio.run(main())
