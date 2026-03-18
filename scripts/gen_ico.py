"""Generate baud.ico from docs/assets/logo.svg using pycairo for accurate rendering."""
import cairo
from PIL import Image
import io, math, os, struct

# All coordinates from logo.svg (512x512 viewBox)
SVG_SIZE = 512

def draw_logo_cairo(size):
    """Render the Baud logo at the given pixel size using pycairo."""
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, size, size)
    ctx = cairo.Context(surface)
    # Scale from 512 SVG coords to target size
    scale = size / SVG_SIZE
    ctx.scale(scale, scale)

    # --- Background gradient rounded square ---
    rx = 96
    ctx.new_path()
    ctx.arc(rx, rx, rx, math.pi, 1.5 * math.pi)           # top-left
    ctx.arc(SVG_SIZE - rx, rx, rx, 1.5 * math.pi, 0)       # top-right
    ctx.arc(SVG_SIZE - rx, SVG_SIZE - rx, rx, 0, 0.5 * math.pi)  # bottom-right
    ctx.arc(rx, SVG_SIZE - rx, rx, 0.5 * math.pi, math.pi) # bottom-left
    ctx.close_path()

    grad = cairo.LinearGradient(0, 0, SVG_SIZE, SVG_SIZE)
    grad.add_color_stop_rgba(0, 124/255, 58/255, 237/255, 1)
    grad.add_color_stop_rgba(1, 6/255, 182/255, 212/255, 1)
    ctx.set_source(grad)
    ctx.fill()

    # --- Subtle inner glow ---
    ctx.new_path()
    irx = 92
    ctx.arc(8 + irx, 8 + irx, irx, math.pi, 1.5 * math.pi)
    ctx.arc(504 - irx, 8 + irx, irx, 1.5 * math.pi, 0)
    ctx.arc(504 - irx, 504 - irx, irx, 0, 0.5 * math.pi)
    ctx.arc(8 + irx, 504 - irx, irx, 0.5 * math.pi, math.pi)
    ctx.close_path()
    ctx.set_source_rgba(1, 1, 1, 0.08)
    ctx.set_line_width(2)
    ctx.stroke()

    # --- B letter gradient ---
    b_grad = cairo.LinearGradient(160, 100, 360, 420)
    b_grad.add_color_stop_rgba(0, 1, 1, 1, 1)
    b_grad.add_color_stop_rgba(1, 232/255, 222/255, 1, 1)

    def rounded_rect(x, y, w, h, r):
        ctx.new_path()
        ctx.arc(x + r, y + r, r, math.pi, 1.5 * math.pi)
        ctx.arc(x + w - r, y + r, r, 1.5 * math.pi, 0)
        ctx.arc(x + w - r, y + h - r, r, 0, 0.5 * math.pi)
        ctx.arc(x + r, y + h - r, r, 0.5 * math.pi, math.pi)
        ctx.close_path()

    # Vertical spine: rect x=148 y=108 w=36 h=296 rx=18
    rounded_rect(148, 108, 36, 296, 18)
    ctx.set_source(b_grad)
    ctx.fill()

    # Top serif: rect x=130 y=108 w=72 h=32 rx=8
    rounded_rect(130, 108, 72, 32, 8)
    ctx.set_source(b_grad)
    ctx.fill()

    # Middle serif: rect x=130 y=240 w=72 h=32 rx=8
    rounded_rect(130, 240, 72, 32, 8)
    ctx.set_source(b_grad)
    ctx.fill()

    # Bottom serif: rect x=130 y=372 w=72 h=32 rx=8
    rounded_rect(130, 372, 72, 32, 8)
    ctx.set_source(b_grad)
    ctx.fill()

    # Top bowl: path "M184 116 H260 C310 116 348 150 348 192 C348 234 310 268 260 268 H184"
    ctx.new_path()
    ctx.move_to(184, 116)
    ctx.line_to(260, 116)
    ctx.curve_to(310, 116, 348, 150, 348, 192)
    ctx.curve_to(348, 234, 310, 268, 260, 268)
    ctx.line_to(184, 268)
    ctx.set_source(b_grad)
    ctx.set_line_width(32)
    ctx.set_line_cap(cairo.LINE_CAP_ROUND)
    ctx.set_line_join(cairo.LINE_JOIN_ROUND)
    ctx.stroke()

    # Bottom bowl: path "M184 248 H272 C328 248 370 286 370 332 C370 378 328 412 272 412 H184"
    ctx.new_path()
    ctx.move_to(184, 248)
    ctx.line_to(272, 248)
    ctx.curve_to(328, 248, 370, 286, 370, 332)
    ctx.curve_to(370, 378, 328, 412, 272, 412)
    ctx.line_to(184, 412)
    ctx.set_source(b_grad)
    ctx.set_line_width(32)
    ctx.set_line_cap(cairo.LINE_CAP_ROUND)
    ctx.set_line_join(cairo.LINE_JOIN_ROUND)
    ctx.stroke()

    # --- Signal dot ---
    sig_grad = cairo.LinearGradient(340, 120, 400, 180)
    sig_grad.add_color_stop_rgba(0, 34/255, 197/255, 94/255, 1)
    sig_grad.add_color_stop_rgba(1, 22/255, 163/255, 74/255, 1)
    ctx.new_path()
    ctx.arc(380, 130, 22, 0, 2 * math.pi)
    ctx.set_source(sig_grad)
    ctx.fill()

    # --- Signal arcs ---
    # Arc 1: "M394 114 C414 100 430 114 414 134"
    ctx.new_path()
    ctx.move_to(394, 114)
    ctx.curve_to(414, 100, 430, 114, 414, 134)
    ctx.set_source_rgba(34/255, 197/255, 94/255, 0.7)
    ctx.set_line_width(5)
    ctx.set_line_cap(cairo.LINE_CAP_ROUND)
    ctx.stroke()

    # Arc 2: "M406 102 C434 82 458 102 434 130"
    ctx.new_path()
    ctx.move_to(406, 102)
    ctx.curve_to(434, 82, 458, 102, 434, 130)
    ctx.set_source_rgba(34/255, 197/255, 94/255, 0.45)
    ctx.set_line_width(4)
    ctx.stroke()

    # Arc 3: "M418 90 C452 64 484 90 454 126"
    ctx.new_path()
    ctx.move_to(418, 90)
    ctx.curve_to(452, 64, 484, 90, 454, 126)
    ctx.set_source_rgba(34/255, 197/255, 94/255, 0.25)
    ctx.set_line_width(3)
    ctx.stroke()

    # Convert cairo surface to PIL Image
    buf = surface.get_data()
    img = Image.frombuffer('RGBA', (size, size), bytes(buf), 'raw', 'BGRA', 0, 1)
    return img


ico_path = os.path.join(os.path.dirname(__file__), '..', 'docs', 'assets', 'baud.ico')
ico_path = os.path.abspath(ico_path)
sizes = [16, 24, 32, 48, 64, 128, 256]
images = [draw_logo_cairo(s) for s in sizes]
print("All sizes rendered")

images[-1].save(ico_path, format='ICO', append_images=images[:-1])
fsize = os.path.getsize(ico_path)
print(f"baud.ico: {fsize} bytes  ({ico_path})")

check = Image.open(ico_path)
print(f"Contains sizes: {check.info.get('sizes', 'unknown')}")

