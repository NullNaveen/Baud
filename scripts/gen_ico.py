"""Generate baud.ico with the official Baud logo design."""
from PIL import Image, ImageDraw
import math, os

def draw_logo(size):
    """Draw Baud logo: gradient rounded rect, serif B, green signal dot + arcs."""
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    s = size
    c1, c2 = (124, 58, 237), (6, 182, 212)
    radius = int(s * 0.16)
    
    # Draw gradient rounded rect
    for y in range(s):
        for x in range(s):
            inx = True
            if x < radius and y < radius and (x-radius)**2+(y-radius)**2 > radius**2: inx = False
            if x > s-1-radius and y < radius and (x-(s-1-radius))**2+(y-radius)**2 > radius**2: inx = False
            if x < radius and y > s-1-radius and (x-radius)**2+(y-(s-1-radius))**2 > radius**2: inx = False
            if x > s-1-radius and y > s-1-radius and (x-(s-1-radius))**2+(y-(s-1-radius))**2 > radius**2: inx = False
            if inx:
                t = (x+y)/(2*s)
                r = int(c1[0]+(c2[0]-c1[0])*t)
                g = int(c1[1]+(c2[1]-c1[1])*t)
                b = int(c1[2]+(c2[2]-c1[2])*t)
                img.putpixel((x,y),(r,g,b,255))
    
    # Serif B
    sx, sw = int(s*0.22), max(1,int(s*0.08))
    st, sb = int(s*0.14), int(s*0.86)
    draw.rectangle([sx,st,sx+sw,sb], fill=(255,255,255,255))
    # Serifs
    sfw = max(1,int(s*0.14))
    sfh = max(1,int(s*0.06))
    draw.rectangle([sx-max(1,int(s*0.02)),st,sx+sfw,st+sfh], fill=(255,255,255,255))
    my = int(s*0.48)
    draw.rectangle([sx-max(1,int(s*0.02)),my-sfh//2,sx+sfw,my+sfh//2], fill=(255,255,255,255))
    draw.rectangle([sx-max(1,int(s*0.02)),sb-sfh,sx+sfw,sb], fill=(255,255,255,255))
    
    # Bowls using ellipse arcs
    bx = sx + sw//2
    # Top bowl
    tc = (st+my)//2
    trx, try_ = int(s*0.22), int((my-st)*0.42)
    draw.arc([bx-1, tc-try_, bx+2*trx, tc+try_], -90, 90, fill=(255,255,255,255), width=max(1,int(s*0.04)))
    # Bottom bowl (larger)
    bc = (my+sb)//2
    brx, bry = int(s*0.28), int((sb-my)*0.42)
    draw.arc([bx-1, bc-bry, bx+2*brx, bc+bry], -90, 90, fill=(255,255,255,255), width=max(1,int(s*0.04)))
    
    # Green signal dot
    dx, dy, dr = int(s*0.72), int(s*0.22), max(1,int(s*0.065))
    draw.ellipse([dx-dr, dy-dr, dx+dr, dy+dr], fill=(34,197,94,255))
    
    # Signal arcs
    for ar_mult in [2.0, 3.0]:
        ar = int(dr * ar_mult)
        draw.arc([dx-ar, dy-ar, dx+ar, dy+ar], -45, 45, fill=(34,197,94,160), width=max(1,int(s*0.015)))
    
    return img

ico_path = r'c:\Users\nickk\Mine\Baud\docs\assets\baud.ico'
sizes = [16, 24, 32, 48, 64, 128, 256]
images = [draw_logo(s) for s in sizes]
print("All sizes rendered")

# Save - largest image first with rest appended
images[-1].save(ico_path, format='ICO', append_images=images[:-1])
fsize = os.path.getsize(ico_path)
print(f"baud.ico: {fsize} bytes")

check = Image.open(ico_path)
print(f"Contains sizes: {check.info.get('sizes', 'unknown')}")

