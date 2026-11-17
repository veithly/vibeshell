#!/usr/bin/env python3
"""Present actual macOS virtual-display screenshots in a README layout.

The translucent window, close controls and contents are copied exclusively
from the original ScreenCaptureKit/screencapture output. Only outside-window
wallpaper, typography, shadow and margins are composed by this script.
"""
from pathlib import Path
import subprocess

from PIL import Image, ImageDraw, ImageFilter, ImageFont


ROOT = Path(__file__).resolve().parents[2]
WALLPAPER = Path(
    "/System/Library/Desktop Pictures/.wallpapers/Sonoma Horizon/Sonoma Horizon.heic"
)
WALLPAPER_PNG = ROOT / "test-results/readme-glass/sonoma-large.png"
CAPTURE_DIR = ROOT / "test-results/readme-glass"
ASSET_DIR = ROOT / "assets/readme"
CANVAS_SIZE = (2800, 1900)
APP_BOUNDS = (110, 44, 1310, 844)
APP_RENDER_SIZE = (2400, 1600)


def font(size: int, bold: bool = False):
    candidates = [
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf"
        if bold else "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/HelveticaNeue.ttc",
    ]
    for candidate in candidates:
        if Path(candidate).exists():
            return ImageFont.truetype(candidate, size)
    return ImageFont.load_default()


def crop_fill(image: Image.Image, size: tuple[int, int]):
    ratio = max(size[0] / image.width, size[1] / image.height)
    resized = image.resize(
        (round(image.width * ratio), round(image.height * ratio)),
        Image.Resampling.LANCZOS,
    )
    left = (resized.width - size[0]) // 2
    top = (resized.height - size[1]) // 2
    return resized.crop((left, top, left + size[0], top + size[1]))


def make_board(wallpaper: Image.Image, capture: Path, output: Path, *, dark: bool):
    if not capture.exists():
        raise FileNotFoundError(f"Native screenshot missing: {capture}")
    screen = Image.open(capture).convert("RGBA")
    if screen.size != (1440, 900):
        raise ValueError(f"Expected virtual-display screenshot 1440x900: {screen.size}")

    # Retain every original UI pixel; no overlay or translucency adjustment
    # is applied inside the native window.
    app = screen.crop(APP_BOUNDS).resize(APP_RENDER_SIZE, Image.Resampling.LANCZOS)
    backdrop = crop_fill(wallpaper, CANVAS_SIZE).filter(
        ImageFilter.GaussianBlur(36)
    ).convert("RGBA")
    backdrop.alpha_composite(
        Image.new("RGBA", CANVAS_SIZE, (9, 14, 22, 149) if dark else (241, 243, 245, 126))
    )

    x, y, radius = 200, 195, 29
    silhouette = Image.new("L", CANVAS_SIZE, 0)
    ImageDraw.Draw(silhouette).rounded_rectangle(
        (x, y + 22, x + app.width, y + app.height + 22),
        radius=radius, fill=165 if dark else 110
    )
    shadow = Image.new("RGBA", CANVAS_SIZE, (0, 0, 0, 0))
    shadow.putalpha(silhouette.filter(ImageFilter.GaussianBlur(41)))
    backdrop.alpha_composite(shadow)

    mask = Image.new("L", app.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (0, 0, app.width - 1, app.height - 1), radius=radius, fill=255
    )
    app.putalpha(mask)
    backdrop.alpha_composite(app, dest=(x, y))

    draw = ImageDraw.Draw(backdrop, "RGBA")
    foreground = (247, 249, 252, 255) if dark else (32, 43, 55, 255)
    muted = (226, 235, 242, 215) if dark else (50, 70, 85, 220)
    draw.rounded_rectangle(
        (x, y, x + app.width - 1, y + app.height - 1), radius=radius,
        outline=(255, 255, 255, 65) if dark else (28, 40, 50, 85), width=2
    )
    draw.rounded_rectangle(
        (159, 75, 215, 131), radius=12, fill=(16, 19, 27, 224),
        outline=(230, 240, 247, 65), width=1
    )
    draw.line(
        [(173, 89), (186, 115), (201, 89)],
        fill=(245, 249, 252, 255), width=6, joint="curve"
    )
    draw.rounded_rectangle(
        (195, 112, 204, 121), radius=2, fill=(99, 224, 202, 255)
    )
    draw.text((236, 65), "VibeShell", font=font(64, bold=True), fill=foreground)
    draw.text(
        (1630, 88),
        "ONE WINDOW. EVERY WORKFLOW." if dark else "WORK YOUR WAY. SIDE BY SIDE.",
        font=font(30), fill=muted
    )
    draw.text(
        (200, 1830), "SSH    /    SFTP    /    PLUGINS    /    PREVIEW",
        font=font(28), fill=muted
    )
    draw.text(
        (2210, 1830),
        "macOS  ·  NIGHT GLASS" if dark else "macOS  ·  DAY GLASS",
        font=font(27), fill=muted
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    backdrop.convert("RGB").save(output, "PNG", optimize=True)
    print(f"Updated {output.relative_to(ROOT)}: {output.stat().st_size} bytes")


def main():
    if not WALLPAPER_PNG.exists():
        subprocess.run(
            ["sips", "-Z", "3000", "-s", "format", "png",
             str(WALLPAPER), "--out", str(WALLPAPER_PNG)],
            stdout=subprocess.DEVNULL, check=True
        )
    wallpaper = Image.open(WALLPAPER_PNG).convert("RGB")
    make_board(
        wallpaper, CAPTURE_DIR / "night-virtual.png",
        ASSET_DIR / "glass-showcase.png", dark=True
    )
    make_board(
        wallpaper, CAPTURE_DIR / "day-virtual.png",
        ASSET_DIR / "panes.png", dark=False
    )


if __name__ == "__main__":
    main()
