"""
FlacCrunch icon — FLAC-style orange badge with a compression down-arrow.
Layout:
  - Orange rounded square background (FLAC brand colour)
  - Subtle audio waveform bars in the background
  - Bold "FLAC" wordmark (top half)
  - Compression arrow (bottom half): down-arrow with horizontal zip lines
    through its shaft, symbolising compressing/crunching
"""
import struct, io, math
from PIL import Image, ImageDraw, ImageFont

SIZE = 1024

# FLAC brand palette
ORANGE_TOP   = (255, 120,  10)
ORANGE_BOT   = (210,  70,   0)
WHITE        = (255, 255, 255)
WHITE_DIM    = (255, 255, 255, 80)
ARROW_WHITE  = (255, 255, 255, 230)
LINE_COLOR   = (255, 200, 130, 140)   # warm tint for zip lines
SHADOW_C     = (130,  40,   0, 120)


def make_icon(size):
    s   = size
    img = Image.new("RGBA", (s, s), (0,0,0,0))

    # ── Orange gradient background ────────────────────────────────────────
    for y in range(s):
        t  = y / (s-1)
        r_ = int(ORANGE_TOP[0] + (ORANGE_BOT[0]-ORANGE_TOP[0])*t)
        g_ = int(ORANGE_TOP[1] + (ORANGE_BOT[1]-ORANGE_TOP[1])*t)
        b_ = int(ORANGE_TOP[2] + (ORANGE_BOT[2]-ORANGE_TOP[2])*t)
        ImageDraw.Draw(img).line([(0,y),(s,y)], fill=(r_,g_,b_,255))

    # Mask to rounded square
    mask = Image.new("L", (s,s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0,0,s-1,s-1], radius=int(s*0.18), fill=255)
    img.putalpha(mask)
    d = ImageDraw.Draw(img)

    # ── Subtle waveform background (decorative) ───────────────────────────
    wave_y  = int(s * 0.73)
    wave_cx = s // 2
    wave_w  = int(s * 0.82)
    pts = []
    for i in range(101):
        t  = i / 100
        px = wave_cx - wave_w//2 + int(wave_w * t)
        py = wave_y + int(math.sin(t * math.pi * 5) * int(s * 0.045))
        pts.append((px, py))
    if len(pts) > 1:
        d.line(pts, fill=(255,255,255,30), width=max(3, s//120))

    # Second waveform slightly below
    pts2 = []
    for i in range(101):
        t  = i / 100
        px = wave_cx - wave_w//2 + int(wave_w * t)
        py = wave_y + int(s*0.06) + int(math.sin(t * math.pi * 5 + 1.2) * int(s*0.03))
        pts2.append((px, py))
    if len(pts2) > 1:
        d.line(pts2, fill=(255,255,255,18), width=max(2, s//160))

    # ── "FLAC" wordmark ───────────────────────────────────────────────────
    font_size = int(s * 0.30)
    try:
        font = ImageFont.truetype("C:/Windows/Fonts/impact.ttf", font_size)
    except Exception:
        try:
            font = ImageFont.truetype("C:/Windows/Fonts/arialbd.ttf", font_size)
        except Exception:
            font = ImageFont.load_default()

    text     = "FLAC"
    # Measure
    tmp      = Image.new("RGBA", (s*2, s*2), (0,0,0,0))
    td       = ImageDraw.Draw(tmp)
    bbox     = td.textbbox((0,0), text, font=font)
    tw, th   = bbox[2]-bbox[0], bbox[3]-bbox[1]
    tx       = (s - tw) // 2 - bbox[0]
    ty       = int(s * 0.10)

    # Drop shadow
    d.text((tx+int(s*.012), ty+int(s*.012)), text, font=font, fill=(130,40,0,140))
    # Main text
    d.text((tx, ty), text, font=font, fill=WHITE)

    # ── Compression / zip arrow ───────────────────────────────────────────
    # A bold downward arrow; shaft filled with horizontal zip-style lines
    # Arrow sits in the lower ~45% of the icon, centred
    ax      = s // 2              # centre x
    shaft_w = int(s * 0.17)       # shaft width
    shaft_t = int(s * 0.44)       # shaft top y
    shaft_b = int(s * 0.70)       # shaft bottom y (where head starts)
    head_w  = int(s * 0.36)       # arrowhead half-width
    head_b  = int(s * 0.90)       # arrowhead tip y

    # Arrow shaft (rounded rect)
    d.rounded_rectangle(
        [ax - shaft_w//2, shaft_t, ax + shaft_w//2, shaft_b],
        radius=int(s*0.02),
        fill=(*WHITE, 220)
    )

    # Horizontal zip/compression lines inside shaft
    line_count = 6
    line_gap   = (shaft_b - shaft_t) // (line_count + 1)
    for i in range(1, line_count + 1):
        ly = shaft_t + i * line_gap
        # Alternate short/long for zipper look
        inset = int(shaft_w * 0.12) if i % 2 == 0 else int(shaft_w * 0.28)
        d.line(
            [ax - shaft_w//2 + inset, ly, ax + shaft_w//2 - inset, ly],
            fill=(200, 80, 0, 200),
            width=max(2, s//160)
        )

    # Arrowhead (triangle)
    head_pts = [
        (ax - head_w//2, shaft_b),
        (ax + head_w//2, shaft_b),
        (ax,             head_b),
    ]
    d.polygon(head_pts, fill=(*WHITE, 220))

    # Thin outline on arrow for crispness
    # shaft outline
    d.rounded_rectangle(
        [ax - shaft_w//2, shaft_t, ax + shaft_w//2, shaft_b],
        radius=int(s*0.02),
        outline=(200, 80, 0, 160), width=max(2, s//160)
    )
    # head outline
    d.line(head_pts + [head_pts[0]], fill=(200, 80, 0, 160), width=max(2, s//160))

    return img


def save_png(img, path):
    img.save(path, format="PNG", optimize=True)
    print(f"  wrote {path}")

def make_ico(images, path):
    chunks = []
    for im in images:
        buf = io.BytesIO(); im.save(buf, format="PNG"); chunks.append(buf.getvalue())
    num    = len(images)
    header = struct.pack("<HHH", 0, 1, num)
    dirs   = b""; offset = 6 + num*16
    for im, data in zip(images, chunks):
        sz = im.size[0]; w = sz if sz<256 else 0
        dirs  += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    with open(path,"wb") as f: f.write(header+dirs+b"".join(chunks))
    print(f"  wrote {path}")

def make_icns(img_1024, path):
    def chunk(t, d): return struct.pack(">4sI", t.encode(), 8+len(d)) + d
    def pb(img, sz):
        im  = img.resize((sz,sz), Image.LANCZOS)
        buf = io.BytesIO(); im.save(buf, format="PNG"); return buf.getvalue()
    chunks = b"".join(chunk(t, pb(img_1024, sz)) for t,sz in [("ic10",1024),("ic09",512),("ic08",256),("ic07",128)])
    with open(path,"wb") as f: f.write(struct.pack(">4sI", b"icns", 8+len(chunks)) + chunks)
    print(f"  wrote {path}")


print("Generating FlacCrunch icon...")
base = make_icon(SIZE)
out  = "src-tauri/icons/"
for sz, name in [(32,"32x32"),(128,"128x128"),(256,"128x128@2x")]:
    save_png(base.resize((sz,sz),Image.LANCZOS), f"{out}{name}.png")
make_ico([base.resize((sz,sz),Image.LANCZOS) for sz in [16,32,48,64,128,256]], f"{out}icon.ico")
make_icns(base, f"{out}icon.icns")
print("Done.")
