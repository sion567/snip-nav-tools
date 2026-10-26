# 生成 SnipNav 应用图标（罗盘主题，深底绿针）
# 依赖：pip install pillow
from PIL import Image, ImageDraw
import math
import os

S = 256
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# 深色圆角底
d.rounded_rectangle([6, 6, S - 6, S - 6], radius=58, fill=(13, 17, 23, 255), outline=(48, 54, 61, 255), width=6)

cx = cy = S // 2
# 罗盘外圈（绿）
d.ellipse([cx - 88, cy - 88, cx + 88, cy + 88], outline=(63, 185, 80, 255), width=10)
# 四向刻度点
for ang in (0, 90, 180, 270):
    a = math.radians(ang)
    x = cx + math.sin(a) * 74
    y = cy - math.cos(a) * 74
    r = 8
    d.ellipse([x - r, y - r, x + r, y + r], fill=(139, 148, 158, 255))
# 指针：右半绿、左半白
d.polygon([(cx, cy - 62), (cx + 20, cy), (cx, cy + 62)], fill=(63, 185, 80, 255))
d.polygon([(cx, cy - 62), (cx - 20, cy), (cx, cy + 62)], fill=(230, 237, 243, 255))
# 中心轴点
d.ellipse([cx - 13, cy - 13, cx + 13, cy + 13], fill=(13, 17, 23, 255), outline=(63, 185, 80, 255), width=4)

out = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons", "icon.ico")
os.makedirs(os.path.dirname(out), exist_ok=True)
img.save(out, sizes=[(256, 256), (64, 64), (48, 48), (32, 32), (16, 16)])
print("written:", os.path.abspath(out))
