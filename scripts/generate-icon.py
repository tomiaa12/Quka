from pathlib import Path
from struct import pack
import zlib


def chunk(tag: bytes, data: bytes) -> bytes:
    return pack(">I", len(data)) + tag + data + pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


def write_png(path: Path, size: int, rgba: list[int]) -> None:
    raw = b"".join(b"\x00" + bytes(rgba[y * size * 4 : (y + 1) * size * 4]) for y in range(size))
    png = b"".join(
        [
            b"\x89PNG\r\n\x1a\n",
            chunk(b"IHDR", pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)),
            chunk(b"IDAT", zlib.compress(raw, 9)),
            chunk(b"IEND", b""),
        ]
    )
    path.write_bytes(png)


def paint_icon(size: int) -> list[int]:
    pixels = [0] * (size * size * 4)
    radius = int(size * 0.22)
    for y in range(size):
        for x in range(size):
            inside = (
                (x >= radius or y >= radius or (x - radius) ** 2 + (y - radius) ** 2 <= radius**2)
                and (
                    x < size - radius
                    or y >= radius
                    or (x - (size - 1 - radius)) ** 2 + (y - radius) ** 2 <= radius**2
                )
                and (
                    x >= radius
                    or y < size - radius
                    or (x - radius) ** 2 + (y - (size - 1 - radius)) ** 2 <= radius**2
                )
                and (
                    x < size - radius
                    or y < size - radius
                    or (x - (size - 1 - radius)) ** 2 + (y - (size - 1 - radius)) ** 2 <= radius**2
                )
            )
            if not inside:
                continue
            t = x / max(size - 1, 1)
            r = int(109 + (61 - 109) * t)
            g = int(131 + (90 - 131) * t)
            b = int(255 + (241 - 255) * t)
            i = (y * size + x) * 4
            pixels[i : i + 4] = [r, g, b, 255]

    cx = cy = int(size * 0.45)
    ring = int(size * 0.16)
    stroke = max(2, size // 16)
    handle = int(size * 0.18)
    for y in range(size):
        for x in range(size):
            d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5
            on_ring = abs(d - ring) <= stroke / 2
            along = (x - cx) + (y - cy)
            on_handle = (
                x >= cx
                and y >= cy
                and abs((x - cx) - (y - cy)) <= stroke
                and ring <= along / 2**0.5 <= ring + handle
            )
            if on_ring or on_handle:
                i = (y * size + x) * 4
                if pixels[i + 3]:
                    pixels[i : i + 4] = [255, 255, 255, 255]
    return pixels


def main() -> None:
    root = Path(__file__).resolve().parents[1] / "src-tauri" / "icons"
    root.mkdir(parents=True, exist_ok=True)
    write_png(root / "app-icon.png", 512, paint_icon(512))
    print(root / "app-icon.png")


if __name__ == "__main__":
    main()
