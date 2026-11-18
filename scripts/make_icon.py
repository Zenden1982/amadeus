# Иконка Amadeus (колба-лампа с янтарной «A»): python scripts/make_icon.py crates/client/assets [превью.png]
# Нужен Pillow. На 16–32 px — без колбы и с толстыми линиями.
import sys, math
from PIL import Image, ImageDraw, ImageFilter
out = sys.argv[1]
NIXIE=(0xFF,0x9A,0x3C); FIL=(0xFF,0xD9,0xA8); BRASS=(0x7A,0x62,0x46); BG=(0x16,0x12,0x0F); EDGE=(0x4A,0x3B,0x2D)

def render(size):
    S = 8  # суперсэмплинг
    W = size*S
    small = size <= 32
    u = W/64.0
    P = lambda x,y: (x*u, y*u)
    img = Image.new('RGBA',(W,W),(0,0,0,0))
    d = ImageDraw.Draw(img)
    r = 14*u
    d.rounded_rectangle([0.5*u,0.5*u,W-0.5*u,W-0.5*u], radius=r, fill=BG+(255,), outline=EDGE+(255,), width=max(1,int(1.5*u)))
    if not small:
        # латунная колба
        pts=[P(15,54),P(15,24)]+[P(32+17*math.cos(math.pi*(1+i/48)),24+17*math.sin(math.pi*(1+i/48))) for i in range(49)]+[P(49,54)]
        d.line(pts, fill=BRASS+(255,), width=int(2.6*u), joint='curve')
        d.line([P(11,54),P(53,54)], fill=BRASS+(255,), width=int(2.6*u))
    if small:
        a=[P(17,54),P(32,10),P(47,54)]; wave=[P(21,39),P(26,39),P(29,32),P(33,46),P(36.5,36),P(38.5,39),P(43,39)]; core=6.5
    else:
        a=[P(22,48),P(32,17),P(42,48)]; wave=[P(24.5,38),P(28,38),P(30,34),P(32.5,42),P(35,36),P(36.5,38),P(39,38)]; core=3.0
    glow = Image.new('RGBA',(W,W),(0,0,0,0)); g=ImageDraw.Draw(glow)
    g.line(a, fill=NIXIE+(200,), width=int(core*2.4*u), joint='curve'); g.line(wave, fill=NIXIE+(200,), width=int(core*2.4*u), joint='curve')
    glow = glow.filter(ImageFilter.GaussianBlur(core*1.6*u))
    img = Image.alpha_composite(img, glow); d=ImageDraw.Draw(img)
    for seg in (a, wave):
        d.line(seg, fill=NIXIE+(255,), width=int(core*u), joint='curve')
        for p in seg:  # круглые концы и стыки
            rr=core*u/2; d.ellipse([p[0]-rr,p[1]-rr,p[0]+rr,p[1]+rr], fill=NIXIE+(255,))
    if not small:
        p=P(32,17); rr=1.3*u; d.ellipse([p[0]-rr,p[1]-rr,p[0]+rr,p[1]+rr], fill=FIL+(255,))
    # маска скругления, чтобы свечение не вылезало за плашку
    mask = Image.new('L',(W,W),0); ImageDraw.Draw(mask).rounded_rectangle([0,0,W-1,W-1], radius=r, fill=255)
    img.putalpha(Image.composite(img.getchannel('A'), Image.new('L',(W,W),0), mask))
    return img.resize((size,size), Image.LANCZOS)

sizes=[16,20,24,32,48,64,128,256]
imgs={s:render(s) for s in sizes}
imgs[256].save(out+'/icon-256.png'); imgs[32].save(out+'/icon-32.png')
imgs[256].save(out+'/icon.ico', sizes=[(s,s) for s in sizes], append_images=[imgs[s] for s in sizes if s!=256])
# превью для глаз
prev = Image.new('RGBA',(600,280),(40,40,40,255)); x=10
for s in sizes:
    prev.paste(imgs[s],(x,10),imgs[s]); x+=s+10
if len(sys.argv) > 2: prev.save(sys.argv[2])
print('ok')
