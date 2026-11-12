"""PureMinecraft 图标：草方块 + 启动箭头。"""
from pathlib import Path

from PIL import Image, ImageDraw

S = 1024
SS = 4
W = S * SS

GRASS = (124, 189, 74)
GRASS_D = (92, 148, 52)
DIRT = (134, 96, 67)
DIRT_D = (108, 76, 52)
CYAN = (90, 220, 255)


def px(v: float) -> int:
    return int(round(v * SS))


def build() -> Image.Image:
    grad = Image.new("RGB", (1, W))
    top, bot = (20, 24, 34), (7, 8, 12)
    for y in range(W):
        t = y / (W - 1)
        grad.putpixel((0, y), tuple(int(top[i] + (bot[i] - top[i]) * t) for i in range(3)))
    grad = grad.resize((W, W))

    mask = Image.new("L", (W, W), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (px(40), px(40), W - px(40), W - px(40)), radius=px(200), fill=255)
    card = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    card.paste(grad, (0, 0), mask)

    d = ImageDraw.Draw(card)
    d.rounded_rectangle((px(40), px(40), W - px(40), W - px(40)),
                        radius=px(200), outline=(124, 189, 74, 90), width=px(8))

    # 等轴草方块
    cx, cy = W // 2, px(500)
    s = px(230)
    top_face = [(cx, cy - s), (cx + s, cy - s // 2), (cx, cy), (cx - s, cy - s // 2)]
    left = [(cx - s, cy - s // 2), (cx, cy), (cx, cy + s), (cx - s, cy + s // 2)]
    right = [(cx + s, cy - s // 2), (cx, cy), (cx, cy + s), (cx + s, cy + s // 2)]

    d.polygon(left, fill=DIRT, outline=DIRT_D, width=px(6))
    d.polygon(right, fill=DIRT_D, outline=DIRT_D, width=px(6))
    d.polygon(top_face, fill=GRASS, outline=GRASS_D, width=px(6))

    # 顶部草皮边
    d.line([(cx - s, cy - s // 2), (cx, cy)], fill=GRASS, width=px(22))
    d.line([(cx + s, cy - s // 2), (cx, cy)], fill=GRASS_D, width=px(22))

    # 像素噪点（MC 感）
    import random
    rnd = random.Random(7)
    for _ in range(120):
        fx = rnd.randint(-s + 20, s - 20)
        fy = rnd.randint(-s // 2, s // 2)
        if abs(fx) + abs(fy) * 2 <= s * 1.1:
            col = (DIRT_D if fy > 0 else GRASS_D) + (90,)
            d.rectangle((cx + fx, cy + fy, cx + fx + px(8), cy + fy + px(8)), fill=col)

    # 播放三角（画在噪点之后，保证可见）
    tcy = cy + s * 0.30
    tri = [(cx - s * 0.22, tcy - s * 0.34), (cx - s * 0.22, tcy + s * 0.34),
           (cx + s * 0.34, tcy)]
    d.polygon(tri, fill=CYAN)

    return card.resize((S, S), Image.LANCZOS)


def main() -> None:
    out = Path(__file__).with_name("icon.ico")
    master = build()
    sizes = [256, 128, 96, 64, 48, 32, 24, 16]
    imgs = [master.resize((s, s), Image.LANCZOS) for s in sizes]
    imgs[0].save(out, format="ICO", sizes=[(s, s) for s in sizes], append_images=imgs[1:])
    master.resize((512, 512), Image.LANCZOS).save(Path(__file__).with_name("pureminecraft.png"))
    print("wrote", out, out.stat().st_size)


if __name__ == "__main__":
    main()
