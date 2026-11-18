# Иконка Amadeus «Осциллограф» без фона: python scripts/make_icon.py crates/client/assets [превью.png]
# Нужен Pillow. Прозрачный фон, только янтарная «A»-всплеск со свечением. Мелкие (<=32): линия толще и шире,
# перекладина прямая — иначе на 16 px волна сливается в кашу.
import sys
from PIL import Image, ImageDraw, ImageFilter
out = sys.argv[1]
NIXIE=(0xFF,0x9A,0x3C); FIL=(0xFF,0xD9,0xA8)

def render(size):
    S = 8  # суперсэмплинг
    W = size*S
    small = size <= 32
    u = W/64.0
    P = lambda x,y: (x*u, y*u)
    img = Image.new('RGBA',(W,W),(0,0,0,0))
    d = ImageDraw.Draw(img)
    if not small:
        trace=[P(7,46),P(18,46),P(32,13),P(46,46),P(57,46)]
        bar=[P(25.5,34),P(29.5,34),P(31,31),P(33,37),P(34.5,34),P(37.5,34)]
        core=4.0
    else:
        trace=[P(3,50),P(15,50),P(32,8),P(49,50),P(61,50)]
        bar=[P(23,36),P(41,36)]
        core=7.0
    glow = Image.new('RGBA',(W,W),(0,0,0,0)); g=ImageDraw.Draw(glow)
    for seg in (trace, bar):
        g.line(seg, fill=NIXIE+(150,), width=int(core*2.0*u), joint='curve')
    glow = glow.filter(ImageFilter.GaussianBlur(core*1.0*u))
    img = Image.alpha_composite(img, glow); d=ImageDraw.Draw(img)
    for seg in (trace, bar):
        d.line(seg, fill=NIXIE+(255,), width=int(core*u), joint='curve')
        for p in seg:  # круглые концы и стыки
            rr=core*u/2; d.ellipse([p[0]-rr,p[1]-rr,p[0]+rr,p[1]+rr], fill=NIXIE+(255,))
    if not small:
        p=P(32,13); rr=1.2*u; d.ellipse([p[0]-rr,p[1]-rr,p[0]+rr,p[1]+rr], fill=FIL+(255,))
    return img.resize((size,size), Image.LANCZOS)

sizes=[16,20,24,32,48,64,128,256]
imgs={s:render(s) for s in sizes}
imgs[256].save(out+'/icon-256.png'); imgs[32].save(out+'/icon-32.png')
imgs[256].save(out+'/icon.ico', sizes=[(s,s) for s in sizes], append_images=[imgs[s] for s in sizes if s!=256])
# превью для глаз: на тёмном и светлом фоне
if len(sys.argv) > 2:
    prev = Image.new('RGBA',(640,560),(20,40,90,255))
    prev.paste((235,235,235,255),(0,280,640,560))
    for row in (0,280):
        x=10
        for s in sizes:
            prev.paste(imgs[s],(x,row+10),imgs[s]); x+=s+10
    prev.save(sys.argv[2])
print('ok')
