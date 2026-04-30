#!/usr/bin/env python3
"""Regenerate Android launcher icon assets with properly centered N."""
import os
import math

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(SCRIPT_DIR)

import cairo
from PIL import Image, ImageDraw, ImageFont

SIZES = {
    'mdpi': 108,
    'hdpi': 162,
    'xhdpi': 216,
    'xxhdpi': 324,
    'xxxhdpi': 432,
}

MAIN_ICON_SIZES = {
    'icon-192': 192,
    'icon-512': 512,
}


def find_font():
    paths = [
        '/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf',
        '/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf',
        '/usr/share/fonts/truetype/freefont/FreeSansBold.ttf',
        '/usr/share/fonts/TTF/DejaVuSans-Bold.ttf',
    ]
    for p in paths:
        if os.path.exists(p):
            return p
    import glob
    matches = glob.glob('/usr/share/fonts/**/*.ttf', recursive=True)
    bold = [m for m in matches if 'bold' in m.lower() or 'Bold' in m]
    if bold:
        return bold[0]
    sans = [m for m in matches if 'sans' in m.lower() or 'Sans' in m]
    if sans:
        return sans[0]
    if matches:
        return matches[0]
    return None


def render_foreground_png(size, font_path):
    font_size = int(size * 0.6)
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    font = ImageFont.truetype(font_path, font_size)

    bbox = draw.textbbox((0, 0), 'N', font=font, anchor='mm')
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]

    # Center the N: use anchor='mm' (middle-middle) at canvas center
    cx = size / 2
    cy = size / 2
    draw.text((cx, cy), 'N', font=font, anchor='mm', fill=(255, 255, 255, 255))

    bbox_out = img.getbbox()
    if bbox_out:
        content_cy = (bbox_out[1] + bbox_out[3]) / 2
        offset = content_cy - size / 2
    else:
        offset = 0

    return img, offset


def render_icon_png(size, font_path):
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Blue circle background
    draw.ellipse([0, 0, size - 1, size - 1], fill=(59, 130, 246, 255))

    # White N centered
    font_size = int(size * 0.6)
    font = ImageFont.truetype(font_path, font_size)
    cx = size / 2
    cy = size / 2
    draw.text((cx, cy), 'N', font=font, anchor='mm', fill=(255, 255, 255, 255))

    return img


def update_svg(filepath, size, template_type):
    font_size = size * 0.6
    cx = size / 2
    # Match what PIL renders: anchor='mm' means visual center at cx,cy
    # For SVG text, y is baseline. We need to convert from visual center to baseline.
    # cap_ratio ≈ 0.72 for typical sans-serif fonts
    cap_ratio = 0.72
    baseline = cx + font_size * cap_ratio * 0.5

    if template_type == 'circle':
        svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}">
  <circle cx="{cx}" cy="{cx}" r="{cx}" fill="#3b82f6"/>
  <text x="{cx}" y="{baseline}" font-family="Arial, sans-serif" font-size="{font_size}" font-weight="bold" fill="#fff" text-anchor="middle">N</text>
</svg>'''
    elif template_type == 'maskable':
        svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}">
  <rect width="{size}" height="{size}" fill="#3b82f6"/>
  <text x="{cx}" y="{baseline}" font-family="Arial, sans-serif" font-size="{font_size}" font-weight="bold" fill="#fff" text-anchor="middle">N</text>
</svg>'''
    else:
        svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}">
  <text x="{cx}" y="{baseline}" font-family="Arial, sans-serif" font-size="{font_size}" font-weight="bold" fill="#fff" text-anchor="middle">N</text>
</svg>'''

    with open(filepath, 'w') as f:
        f.write(svg)


def main():
    font_path = find_font()
    if not font_path:
        print("ERROR: No TTF font found for rendering", file=sys.stderr)
        sys.exit(1)
    print(f"Using font: {font_path}")

    print("=== Generating ic_launcher_foreground.png ===")
    for density, size in SIZES.items():
        img, offset = render_foreground_png(size, font_path)
        out_dir = os.path.join(PROJECT_ROOT, 'android', 'res', f'mipmap-{density}')
        os.makedirs(out_dir, exist_ok=True)
        out_path = os.path.join(out_dir, 'ic_launcher_foreground.png')
        img.save(out_path, 'PNG')
        print(f"  {density}: {size}x{size} center_offset={offset:+.1f}px")

    print("\n=== Generating public/icons/icon-*.png ===")
    for name, size in MAIN_ICON_SIZES.items():
        img = render_icon_png(size, font_path)
        out_path = os.path.join(PROJECT_ROOT, 'public', 'icons', f'{name}.png')
        img.save(out_path, 'PNG')
        print(f"  {name}.png: {size}x{size}")

    print("\n=== Updating SVG source files ===")
    update_svg(os.path.join(PROJECT_ROOT, 'public', 'icons', 'icon-192.svg'), 192, 'circle')
    update_svg(os.path.join(PROJECT_ROOT, 'public', 'icons', 'icon-512.svg'), 512, 'circle')
    update_svg(os.path.join(PROJECT_ROOT, 'public', 'icons', 'icon-maskable-512.svg'), 512, 'maskable')
    print("  Updated icon-192.svg, icon-512.svg, icon-maskable-512.svg")

    print("\nDone!")


if __name__ == '__main__':
    main()
