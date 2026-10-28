import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SIZE = 1024;
const SS = 2;

const crcTable = new Uint32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  crcTable[n] = c >>> 0;
}

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const typeBuffer = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuffer, data])));
  return Buffer.concat([length, typeBuffer, data, crc]);
}

function encodePng(pixels, size) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header[8] = 8;
  header[9] = 6;
  header[10] = 0;
  header[11] = 0;
  header[12] = 0;

  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) {
    const rowStart = y * (size * 4 + 1);
    raw[rowStart] = 0;
    pixels.copy(raw, rowStart + 1, y * size * 4, (y + 1) * size * 4);
  }

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function smoothstep(edge0, edge1, value) {
  const t = Math.min(1, Math.max(0, (value - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

function roundedRectSdf(x, y, cx, cy, halfW, halfH, radius) {
  const qx = Math.abs(x - cx) - (halfW - radius);
  const qy = Math.abs(y - cy) - (halfH - radius);
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  return outside + Math.min(Math.max(qx, qy), 0) - radius;
}

function insideTriangle(px, py, ax, ay, bx, by, cx, cy) {
  const d1 = (px - bx) * (ay - by) - (ax - bx) * (py - by);
  const d2 = (px - cx) * (by - cy) - (bx - cx) * (py - cy);
  const d3 = (px - ax) * (cy - ay) - (cx - ax) * (py - ay);
  const hasNeg = d1 < 0 || d2 < 0 || d3 < 0;
  const hasPos = d1 > 0 || d2 > 0 || d3 > 0;
  return !(hasNeg && hasPos);
}

function sample(x, y) {
  const nx = x / SIZE;
  const ny = y / SIZE;

  const bgT = Math.min(1, Math.max(0, (nx * 0.45 + ny * 0.55)));
  let r = 11 + (16 - 11) * bgT;
  let g = 15 + (56 - 15) * bgT;
  let b = 24 + (48 - 24) * bgT;

  const glowDistance = Math.hypot(x - SIZE * 0.72, y - SIZE * 0.24);
  const glow = Math.max(0, 1 - glowDistance / (SIZE * 0.55));
  r += 10 * glow;
  g += 88 * glow;
  b += 62 * glow;

  if (insideTriangle(x, y, SIZE * 0.5, SIZE * 0.86, SIZE * 0.3, SIZE * 0.52, SIZE * 0.7, SIZE * 0.52)) {
    r = 240;
    g = 255;
    b = 249;
  }
  if (x > SIZE * 0.435 && x < SIZE * 0.565 && y > SIZE * 0.2 && y < SIZE * 0.56) {
    r = 240;
    g = 255;
    b = 249;
  }
  if (x > SIZE * 0.34 && x < SIZE * 0.66 && y > SIZE * 0.64 && y < SIZE * 0.68) {
    r = 240;
    g = 255;
    b = 249;
  }

  return [r, g, b];
}

const pixels = Buffer.alloc(SIZE * SIZE * 4);
for (let y = 0; y < SIZE; y++) {
  for (let x = 0; x < SIZE; x++) {
    let alpha = 0;
    let red = 0;
    let green = 0;
    let blue = 0;
    for (let sy = 0; sy < SS; sy++) {
      for (let sx = 0; sx < SS; sx++) {
        const px = x + (sx + 0.5) / SS;
        const py = y + (sy + 0.5) / SS;
        const sdf = roundedRectSdf(px, py, SIZE / 2, SIZE / 2, SIZE * 0.47, SIZE * 0.47, SIZE * 0.22);
        const coverage = 1 - smoothstep(-1, 1, sdf);
        alpha += coverage;
        const [r, g, b] = sample(px, py);
        red += r * coverage;
        green += g * coverage;
        blue += b * coverage;
      }
    }
    const total = SS * SS;
    const a = alpha / total;
    const offset = (y * SIZE + x) * 4;
    pixels[offset] = a > 0 ? Math.round(red / alpha) : 0;
    pixels[offset + 1] = a > 0 ? Math.round(green / alpha) : 0;
    pixels[offset + 2] = a > 0 ? Math.round(blue / alpha) : 0;
    pixels[offset + 3] = Math.round(a * 255);
  }
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const target = resolve(scriptDir, "..", "assets", "icon-source.png");
mkdirSync(dirname(target), { recursive: true });
writeFileSync(target, encodePng(pixels, SIZE));
console.log(`wrote ${target}`);
