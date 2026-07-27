#!/usr/bin/env python3

import csv
import re
import sys

import matplotlib.animation as animation
import matplotlib.pyplot as plt
import numpy as np


def time_ms(header: str) -> float:
    m = re.search(r"@(\d+)\[ns\]", header)
    return float(m.group(1)) / 1_000_000 if m else 0.0


def main() -> None:
    if len(sys.argv) < 2:
        sys.exit("usage: python3 plot_field.py <csv>")
    path = sys.argv[1]

    with open(path, newline="") as f:
        reader = csv.reader(f)
        header = next(reader)
        data = np.array([[float(c) for c in row] for row in reader])

    cols = {name: i for i, name in enumerate(header)}
    xi, yi, zi = cols["x[mm]"], cols["y[mm]"], cols["z[mm]"]
    value_cols = list(range(zi + 1, len(header)))
    if not value_cols:
        sys.exit("no value columns found in CSV")

    x = np.unique(data[:, xi])
    y = np.unique(data[:, yi])
    extent = [x.min(), x.max(), y.min(), y.max()]

    def grid(col: int) -> np.ndarray:
        return data[:, col].reshape(len(y), len(x))

    fig, ax = plt.subplots()
    ax.set_xlabel("x [mm]")
    ax.set_ylabel("y [mm]")

    if len(value_cols) == 1:
        col = value_cols[0]
        im = ax.imshow(
            grid(col), origin="lower", aspect="equal", extent=extent, cmap="jet"
        )
        ax.set_title(header[col])
        fig.colorbar(im, ax=ax)
        plt.show()
        return

    vmax = float(np.max(np.abs(data[:, value_cols])))
    im = ax.imshow(
        grid(value_cols[0]),
        origin="lower",
        aspect="equal",
        extent=extent,
        cmap="RdBu_r",
        vmin=-vmax,
        vmax=vmax,
    )
    fig.colorbar(im, ax=ax)

    def update(frame: int):
        col = value_cols[frame]
        im.set_data(grid(col))
        ax.set_title(f"p [Pa]  t = {time_ms(header[col]):.4f} ms")
        return (im,)

    anim = animation.FuncAnimation(
        fig, update, frames=len(value_cols), interval=80, blit=False, repeat=True
    )
    fig._autd3_anim = anim
    plt.show()


if __name__ == "__main__":
    main()
