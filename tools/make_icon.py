"""Generate the original PulseDeck vector-style app mark as an RGBA PNG."""
from pathlib import Path
import struct, zlib

size = 256
rows = bytearray()
amplitudes = [0.22, 0.44, 0.68, 0.9, 0.54, 0.74, 0.38]
for y in range(size):
    rows.append(0)
    for x in range(size):
        u, v = (x + .5) / size, (y + .5) / size
        qx, qy = max(abs(u-.5)-.34,0),max(abs(v-.5)-.34,0)
        inside = qx*qx+qy*qy < .13*.13
        r,g,b,a = 19,24,33,255 if inside else 0
        if inside:
            g += int(12*(1-v)); b += int(15*(1-v))
            for i,h in enumerate(amplitudes):
                cx=.23+i*.09; half=.028
                dy=max(abs(v-.5)-h*.25,0)
                dx=max(abs(u-cx)-half*.6,0)
                if dx*dx+dy*dy<half*half*.16:
                    blend=i/6
                    r=int(244*(1-blend)+98*blend);g=int(182*(1-blend)+203*blend);b=int(91*(1-blend)+213*blend)
        rows.extend([r,g,b,a])
def chunk(kind, payload):
    return struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload)&0xffffffff)
image=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',size,size,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(rows,9))+chunk(b'IEND',b'')
dest=Path(__file__).resolve().parents[1]/'src-tauri/icons/icon.png'
dest.parent.mkdir(parents=True,exist_ok=True)
dest.write_bytes(image)
print(dest)
