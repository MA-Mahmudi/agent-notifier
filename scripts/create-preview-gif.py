#!/usr/bin/env python3
"""Generate the animated Agent Notifier README preview."""

from __future__ import annotations

import math
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


WIDTH = 800
HEIGHT = 500
FRAMES = 36
FRAME_MS = 125

COLORS = {
    "background": "#0d0f13",
    "topbar": "#17191e",
    "panel": "#1c1f24",
    "card": "#25282d",
    "card_border": "#3b3e44",
    "text": "#f6f5f4",
    "muted": "#a8a8ad",
    "blue": "#62a0ea",
    "red": "#f66151",
    "green": "#57e389",
    "button": "#34373d",
}


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"
    path = Path("/usr/share/fonts/truetype/dejavu") / name
    return ImageFont.truetype(str(path), size)


FONT_12 = font(12)
FONT_13 = font(13)
FONT_13_BOLD = font(13, True)
FONT_15 = font(15)
FONT_15_BOLD = font(15, True)
FONT_19_BOLD = font(19, True)


def text(draw: ImageDraw.ImageDraw, xy: tuple[int, int], value: str, *,
         fill: str = COLORS["text"], face: ImageFont.FreeTypeFont = FONT_13) -> None:
    draw.text(xy, value, font=face, fill=fill)


def button(draw: ImageDraw.ImageDraw, box: tuple[int, int, int, int], label: str,
           *, accent: str | None = None, icon: str = "") -> None:
    fill = COLORS["button"] if accent is None else blend(COLORS["button"], accent, 0.28)
    draw.rounded_rectangle(box, radius=9, fill=fill, outline=accent or COLORS["card_border"], width=1)
    x1, y1, _, _ = box
    if icon:
        text(draw, (x1 + 10, y1 + 8), icon, fill=accent or COLORS["muted"], face=FONT_13_BOLD)
        text(draw, (x1 + 30, y1 + 8), label, fill=accent or COLORS["text"], face=FONT_13_BOLD)
    else:
        text(draw, (x1 + 12, y1 + 8), label, fill=accent or COLORS["text"], face=FONT_13_BOLD)


def blend(first: str, second: str, amount: float) -> tuple[int, int, int]:
    a = tuple(int(first[index:index + 2], 16) for index in (1, 3, 5))
    b = tuple(int(second[index:index + 2], 16) for index in (1, 3, 5))
    return tuple(round(x + (y - x) * amount) for x, y in zip(a, b))


def session_card(draw: ImageDraw.ImageDraw, top: int, *, agent: str, project: str,
                 title: str, preview: str, state: str, state_color: str,
                 notify: bool, highlighted: bool = False) -> None:
    left, right = 96, 704
    outline = state_color if highlighted else COLORS["card_border"]
    draw.rounded_rectangle((left, top, right, top + 132), radius=14,
                           fill=COLORS["card"], outline=outline, width=2 if highlighted else 1)
    draw.rounded_rectangle((left, top, left + 4, top + 132), radius=2, fill=state_color)
    text(draw, (left + 18, top + 15), f"{agent}  —  {project}/{title}  —  {state}",
         face=FONT_15_BOLD)
    text(draw, (left + 18, top + 48), preview, fill="#deddda", face=FONT_13)
    draw.ellipse((left + 18, top + 82, left + 30, top + 94), fill=state_color)
    text(draw, (left + 38, top + 80), state, fill=state_color, face=FONT_13_BOLD)
    button(draw, (right - 210, top + 73, right - 104, top + 108),
           "Notify on" if notify else "Notify off",
           accent=COLORS["green"] if notify else None,
           icon="●" if notify else "○")
    button(draw, (right - 94, top + 73, right - 18, top + 108), "Hide", icon="–")


def render_frame(index: int) -> Image.Image:
    image = Image.new("RGB", (WIDTH, HEIGHT), COLORS["background"])
    draw = ImageDraw.Draw(image)

    if index < 12:
        state, state_color, top_label = "Working", COLORS["blue"], "codex · agent-notifier"
        notify, phase = False, "Live running sessions stay visible in the top bar"
    elif index < 24:
        state, state_color, top_label = "Attention", COLORS["red"], "codex · agent-notifier"
        notify, phase = False, "Attention is shown immediately with a red status"
    else:
        state, state_color, top_label = "Done", COLORS["green"], "codex · agent-notifier"
        notify, phase = True, "Completion alerts can be enabled per session or globally"

    draw.rectangle((0, 0, WIDTH, 44), fill=COLORS["topbar"])
    pulse = 0.45 + 0.45 * (1 + math.sin(index * math.pi / 2)) / 2
    chip_fill = blend(COLORS["topbar"], state_color, 0.24)
    draw.rounded_rectangle((283, 7, 517, 37), radius=16, fill=chip_fill,
                           outline=blend(COLORS["topbar"], state_color, 0.55), width=1)
    dot_color = blend(COLORS["topbar"], state_color, pulse) if state == "Working" else state_color
    draw.ellipse((298, 17, 308, 27), fill=dot_color)
    text(draw, (317, 12), top_label, face=FONT_15)

    draw.rounded_rectangle((76, 62, 724, 468), radius=18, fill=COLORS["panel"],
                           outline=COLORS["card_border"], width=1)
    text(draw, (96, 82), "Agent Notifier", face=FONT_19_BOLD)
    text(draw, (96, 108), "1 active · 3 recent · 0 hidden · 24h", fill=COLORS["muted"])
    draw.ellipse((679, 87, 693, 101), outline=COLORS["muted"], width=2)

    button(draw, (96, 132, 206, 169), "Enable all", accent=COLORS["green"], icon="●")
    button(draw, (216, 132, 330, 169), "Disable all", icon="○")
    button(draw, (340, 132, 448, 169), "Hidden (0)", icon="–")

    session_card(
        draw,
        184,
        agent="codex",
        project="agent-notifier",
        title="Release pipeline",
        preview="Building packages and checking the release…",
        state=state,
        state_color=state_color,
        notify=notify,
        highlighted=True,
    )
    session_card(
        draw,
        326,
        agent="claude",
        project="website",
        title="Update landing page",
        preview="Final response is ready for review.",
        state="Done",
        state_color=COLORS["green"],
        notify=True,
    )

    draw.rounded_rectangle((128, 447, 672, 487), radius=12, fill="#111318")
    text(draw, (147, 458), phase, fill=COLORS["text"], face=FONT_13_BOLD)
    return image


def main() -> None:
    output = Path(sys.argv[1] if len(sys.argv) > 1 else "assets/agent-notifier-preview.gif")
    output.parent.mkdir(parents=True, exist_ok=True)
    frames = [render_frame(index).quantize(colors=96, method=Image.Quantize.MEDIANCUT)
              for index in range(FRAMES)]
    frames[0].save(
        output,
        save_all=True,
        append_images=frames[1:],
        duration=FRAME_MS,
        loop=0,
        optimize=True,
        disposal=2,
    )
    print(output)


if __name__ == "__main__":
    main()
