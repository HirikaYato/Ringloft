#!/usr/bin/env python3
"""Складывает .ico из готовых PNG.

Зачем своё: `tauri icon` собирает все кадры из одной картинки, поэтому
мелкие (16-32) выходят из тонких линий крупного знака и мажутся. Здесь
кадры берутся каждый от своего варианта логотипа.

Формат простой: заголовок, таблица кадров по 16 байт, дальше сами PNG —
Windows их понимает начиная с Vista.
"""

import struct
import sys
from pathlib import Path


def pack(target: Path, frames: list[tuple[int, Path]]) -> None:
    blobs = [(size, path.read_bytes()) for size, path in frames]

    header = struct.pack("<HHH", 0, 1, len(blobs))
    offset = len(header) + 16 * len(blobs)
    table = b""
    for size, blob in blobs:
        # 256 в таблице пишется нулём: на размер отведён один байт.
        table += struct.pack(
            "<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(blob), offset
        )
        offset += len(blob)

    target.write_bytes(header + table + b"".join(blob for _, blob in blobs))


if __name__ == "__main__":
    if len(sys.argv) < 4 or len(sys.argv) % 2 != 0:
        raise SystemExit("применение: pack-ico.py out.ico размер файл [размер файл …]")

    pairs = sys.argv[2:]
    pack(
        Path(sys.argv[1]),
        [(int(pairs[i]), Path(pairs[i + 1])) for i in range(0, len(pairs), 2)],
    )
