#!/usr/bin/env python3
"""从 logo 生成填满画布的 app-icon.png，避免 Dock 图标显得过小。"""
from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SRC = ROOT.parent / "logo.png"
DEFAULT_OUT = ROOT / "app-icon.png"
SIZE = 1024
FILL = 0.94


def main() -> None:
    src = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_SRC
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else DEFAULT_OUT
    fill = float(sys.argv[3]) if len(sys.argv) > 3 else FILL

    img = Image.open(src).convert("RGBA")
    alpha = np.array(img)[:, :, 3]
    ys, xs = np.where(alpha > 16)
    if len(xs) == 0:
        raise SystemExit(f"no opaque content in {src}")

    cropped = img.crop((xs.min(), ys.min(), xs.max() + 1, ys.max() + 1))
    target = int(SIZE * fill)
    scale = target / max(cropped.width, cropped.height)
    new_w = max(1, int(cropped.width * scale))
    new_h = max(1, int(cropped.height * scale))
    resized = cropped.resize((new_w, new_h), Image.Resampling.LANCZOS)

    canvas = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    canvas.paste(resized, ((SIZE - new_w) // 2, (SIZE - new_h) // 2), resized)
    out.parent.mkdir(parents=True, exist_ok=True)
    canvas.save(out)
    print(f"wrote {out} ({new_w}x{new_h} in {SIZE}x{SIZE})")


if __name__ == "__main__":
    main()
