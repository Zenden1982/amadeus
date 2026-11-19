# Значок Murmur «M-волна с глазами» без фона: python scripts/make_icon.py crates/client/assets [превью.png]
# Нужен Pillow. Линия звука, из которой вырастает M (пики — уши), две точки-глаза (design/logo.svg).
# Мелкие (<=32): линия толще, иначе на 16 px знак рассыпается.
import sys
from PIL import Image, ImageDraw
out = sys.argv[1]
ACCENT = (0x5B, 0x8C, 0xFF)

LINE = [(2, 42), (10, 42), (14, 18), (24, 32), (32, 22), (40, 32), (50, 18), (54, 42), (62, 42)]
EYES = [(26, 44), (38, 44)]
# мелкие (трей, панель задач): та же «M с глазами», но выше и уже — почти квадрат, без «хвостов» линии
LINE_SMALL = [(12, 46), (16, 12), (26, 30), (32, 20), (38, 30), (48, 12), (52, 46)]
EYES_SMALL = [(26, 52), (38, 52)]

def render(size):
    S = 8  # суперсэмплинг
    W = size * S
    small = size <= 32
    u = W / 70.0  # поле по краям: круглые концы линии не обрезаются
    off = 3 * u
    img = Image.new('RGBA', (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # мелкие: обрезаем «хвосты» линии (слишком тонкие) и сдвигаем знак вверх-по центру
    # значок везде компактный (почти квадрат): трей, панель задач, «Пуск»; длинная форма — логотип внутри программы
    line = LINE_SMALL
    eyes = EYES_SMALL
    dy = -4 if small else -4
    pts = [(off + x * u, (y + dy) * u + off + (6 * u if small else 0)) for x, y in line]
    core = (8.5 if small else 7.0) * u
    d.line(pts, fill=ACCENT + (255,), width=int(core), joint='curve')
    for p in pts:  # круглые концы и стыки
        r = core / 2
        d.ellipse([p[0] - r, p[1] - r, p[0] + r, p[1] + r], fill=ACCENT + (255,))
    er = (4.0 if small else 3.6) * u
    for x, y in eyes:
        cx, cy = off + x * u, (y + dy) * u + off + (6 * u if small else 0) + (2 * u if small else 0)
        d.ellipse([cx - er, cy - er, cx + er, cy + er], fill=ACCENT + (255,))
    # подогнать знак почти вплотную к краям (в трее значок мелкий — пустые поля его «съедают»)
    box = img.getbbox()
    if box:
        img = img.crop(box)
        side = max(img.size)
        margin = int(side * (0.03 if small else 0.06))
        sq = Image.new('RGBA', (side + 2 * margin, side + 2 * margin), (0, 0, 0, 0))
        sq.paste(img, (margin + (side - img.size[0]) // 2, margin + (side - img.size[1]) // 2), img)
        img = sq
    # плитка: синий скруглённый квадрат во весь значок, знак белый (как у Discord/Telegram — читается крупно)
    tile = Image.new('RGBA', (W, W), (0, 0, 0, 0))
    td = ImageDraw.Draw(tile)
    pad = int(W * (0.02 if small else 0.04))
    td.rounded_rectangle([pad, pad, W - 1 - pad, W - 1 - pad], radius=int(W * 0.24), fill=ACCENT + (255,))
    white = Image.new('RGBA', img.size, (255, 255, 255, 0))
    white.putalpha(img.getchannel('A'))
    mark_side = int(W * (0.66 if small else 0.60))
    white = white.resize((mark_side, mark_side), Image.LANCZOS)
    off2 = (W - mark_side) // 2
    tile.paste(white, (off2, off2 + int(W * 0.01)), white)
    img = tile
    return img.resize((size, size), Image.LANCZOS)

sizes = [16, 20, 24, 32, 48, 64, 128, 256]
imgs = {s: render(s) for s in sizes}
imgs[256].save(out + '/icon-256.png'); imgs[32].save(out + '/icon-32.png')
imgs[256].save(out + '/icon.ico', sizes=[(s, s) for s in sizes], append_images=[imgs[s] for s in sizes if s != 256])
if len(sys.argv) > 2:
    prev = Image.new('RGBA', (640, 560), (17, 19, 23, 255))
    prev.paste((242, 243, 246, 255), (0, 280, 640, 560))
    for row in (0, 280):
        x = 10
        for s in sizes:
            prev.paste(imgs[s], (x, row + 10), imgs[s]); x += s + 10
    prev.save(sys.argv[2])
print('ok')
