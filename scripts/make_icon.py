# Иконка Amadeus «Осциллограф»: python scripts/make_icon.py crates/client/assets [превью.png]
# Нужен Pillow. Крупные (>=48): экран с сеткой, тёплое пятно, волна-перекладина. Мелкие (<=32): без сетки,
# линия толще и шире, перекладина прямая — иначе на 16 px волна сливается в кашу.
import sys
from PIL import Image, ImageDraw, ImageFilter
out = sys.argv[1]
NIXIE=(0xFF,0x9A,0x3C); FIL=(0xFF,0xD9,0xA8); BG=(0x16,0x12,0x0F); WARM=(0x3A,0x26,0x14); GRID=(0x2E,0x24,0x19); EDGE=(0x4A,0x3B,0x2D)

def render(size):
    S = 8  # суперсэмплинг
    W = size*S
    small = size <= 32
    u = W/64.0
    P = lambda x,y: (x*u, y*u)
    img = Image.new('RGBA',(W,W),(0,0,0,0))
    d = ImageDraw.Draw(img)
    r = 14*u
    d.rounded_rectangle([0.5*u,0.5*u,W-0.5*u,W-0.5*u], radius=r, fill=BG+(255,))
    if not small:
        # радиальный градиент: тёплый центр чуть ниже середины
        warm = Image.new('RGBA',(W,W),(0,0,0,0)); ImageDraw.Draw(warm).ellipse([P(8,13)[0],P(8,13)[1],P(56,61)[0],P(56,61)[1]], fill=WARM+(255,))
        img = Image.alpha_composite(img, warm.filter(ImageFilter.GaussianBlur(10*u))); d = ImageDraw.Draw(img)
        for k in range(5):
            y = 14+10*k; x = 12+10*k
            d.line([P(6,y),P(58,y)], fill=GRID+(255,), width=max(1,int(0.7*u)))
            d.line([P(x,6),P(x,58)], fill=GRID+(255,), width=max(1,int(0.7*u)))
        trace=[P(7,46),P(18,46),P(32,13),P(46,46),P(57,46)]
        bar=[P(25.5,34),P(29.5,34),P(31,31),P(33,37),P(34.5,34),P(37.5,34)]
        core=3.2
    else:
        trace=[P(4,48),P(16,48),P(32,10),P(48,48),P(60,48)]
        bar=[P(23.5,35),P(40.5,35)]
        core=6.0
    d.rounded_rectangle([0.5*u,0.5*u,W-0.5*u,W-0.5*u], radius=r, outline=EDGE+(255,), width=max(1,int(1.5*u)))
    glow = Image.new('RGBA',(W,W),(0,0,0,0)); g=ImageDraw.Draw(glow)
    for seg in (trace, bar):
        g.line(seg, fill=NIXIE+(200,), width=int(core*2.4*u), joint='curve')
    glow = glow.filter(ImageFilter.GaussianBlur(core*1.4*u))
    img = Image.alpha_composite(img, glow); d=ImageDraw.Draw(img)
    for seg in (trace, bar):
        d.line(seg, fill=NIXIE+(255,), width=int(core*u), joint='curve')
        for p in seg:  # круглые концы и стыки
            rr=core*u/2; d.ellipse([p[0]-rr,p[1]-rr,p[0]+rr,p[1]+rr], fill=NIXIE+(255,))
    if not small:
        p=P(32,13); rr=1.2*u; d.ellipse([p[0]-rr,p[1]-rr,p[0]+rr,p[1]+rr], fill=FIL+(255,))
    # маска скругления, чтобы свечение не вылезало за плашку
    mask = Image.new('L',(W,W),0); ImageDraw.Draw(mask).rounded_rectangle([0,0,W-1,W-1], radius=r, fill=255)
    img.putalpha(Image.composite(img.getchannel('A'), Image.new('L',(W,W),0), mask))
    return img.resize((size,size), Image.LANCZOS)

sizes=[16,20,24,32,48,64,128,256]
imgs={s:render(s) for s in sizes}
imgs[256].save(out+'/icon-256.png'); imgs[32].save(out+'/icon-32.png')
imgs[256].save(out+'/icon.ico', sizes=[(s,s) for s in sizes], append_images=[imgs[s] for s in sizes if s!=256])
# превью для глаз: на тёмном и светлом фоне
if len(sys.argv) > 2:
    prev = Image.new('RGBA',(640,560),(40,40,40,255))
    prev.paste((235,235,235,255),(0,280,640,560))
    for row in (0,280):
        x=10
        for s in sizes:
            prev.paste(imgs[s],(x,row+10),imgs[s]); x+=s+10
    prev.save(sys.argv[2])
print('ok')
