"""Generate the original vector icon's PNG/ICO assets with Pillow (no network)."""
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1]
size, scale = 2048, 4
image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
draw = ImageDraw.Draw(image)
def box(coords):
    return tuple(round(v * scale) for v in coords)
draw.rounded_rectangle(box((16, 16, 496, 496)), radius=108*scale, fill="#132B49", outline="#254E75", width=8*scale)
for x, body, top, lines in [(66, "#329FBB", "#79E4E7", "#82E6E4"), (302, "#DCE7F4", "#FFFFFF", "#A3BBD5")]:
    draw.rectangle(box((x, 158, x+144, 326)), fill=body)
    draw.ellipse(box((x, 288, x+144, 364)), fill=body)
    draw.ellipse(box((x, 120, x+144, 196)), fill=top)
    for y in (206, 256):
        draw.arc(box((x, y, x+144, y+76)), start=0, end=180, fill=lines, width=8*scale)
for points, color in [([(224,225),(288,225)],"#FFFFFF"), ([(274,211),(288,225),(274,239)],"#FFFFFF"), ([(288,285),(224,285)],"#79E4E7"), ([(238,271),(224,285),(238,299)],"#79E4E7")]:
    draw.line([box(point) for point in points], fill=color, width=13*scale, joint="curve")
    for x,y in points:
        radius=6.5
        draw.ellipse(box((x-radius,y-radius,x+radius,y+radius)),fill=color)
assets=root/"assets"
assets.mkdir(exist_ok=True)
for pixels in (64,256,512):
    name="schema-compare.png" if pixels==256 else f"schema-compare-{pixels}.png"
    image.resize((pixels,pixels),Image.Resampling.LANCZOS).save(assets/name)
image.resize((256,256),Image.Resampling.LANCZOS).save(assets/"schema-compare.ico",sizes=[(n,n) for n in (16,24,32,48,64,128,256)])
print("Generated original Schema Compare PNG and multi-resolution ICO")
