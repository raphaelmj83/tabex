"""Gera o ícone do Tabex (pasta azul com três abas) em PNG, RGBA bruto e ICO."""
from PIL import Image, ImageDraw

def draw(size: int) -> Image.Image:
    s = 16  # supersampling
    W = size * s
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    u = W / 64.0  # unidade
    back = (0x1E, 0x5F, 0xC2, 255)
    front = (0x3B, 0x8B, 0xF0, 255)
    light = (0x6F, 0xB2, 0xFF, 255)
    r = int(4 * u)
    # aba superior esquerda
    d.rounded_rectangle([6 * u, 10 * u, 30 * u, 22 * u], radius=r, fill=back)
    # corpo de trás
    d.rounded_rectangle([6 * u, 16 * u, 58 * u, 54 * u], radius=r, fill=back)
    # frente
    d.rounded_rectangle([6 * u, 26 * u, 58 * u, 54 * u], radius=r, fill=front)
    # três "abas" claras no topo da frente (grupos de abas)
    for i, c in enumerate([light, (0xFF, 0xB0, 0x3B, 255), (0x4C, 0xD9, 0x7B, 255)]):
        x0 = 12 * u + i * 14 * u
        d.rounded_rectangle([x0, 29 * u, x0 + 11 * u, 33 * u], radius=int(1.5 * u), fill=c)
    return img.resize((size, size), Image.LANCZOS)

sizes = [16, 24, 32, 48, 64, 128, 256]
imgs = {n: draw(n) for n in sizes}
imgs[256].save("assets/icon-256.png")
imgs[64].save("assets/icon-64.png")
with open("assets/icon-64.rgba", "wb") as f:
    f.write(imgs[64].tobytes())
imgs[256].save("assets/tabex.ico", sizes=[(n, n) for n in sizes], append_images=[imgs[n] for n in sizes if n != 256])
print("ok", [img.size for img in imgs.values()])
