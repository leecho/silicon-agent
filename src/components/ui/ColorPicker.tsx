import { useEffect, useRef, useState } from "react";

// 可复用取色器：饱和度/明度方块 + 色相滑条 + 十六进制输入 + 预览块。
// value/onChange 均为 #RRGGBB 十六进制串。

interface Hsv {
  h: number; // 0-360
  s: number; // 0-1
  v: number; // 0-1
}

function clamp01(n: number): number {
  return Math.max(0, Math.min(1, n));
}

function hsvToRgb(h: number, s: number, v: number): [number, number, number] {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  let r = 0;
  let g = 0;
  let b = 0;
  if (h < 60) [r, g, b] = [c, x, 0];
  else if (h < 120) [r, g, b] = [x, c, 0];
  else if (h < 180) [r, g, b] = [0, c, x];
  else if (h < 240) [r, g, b] = [0, x, c];
  else if (h < 300) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  return [
    Math.round((r + m) * 255),
    Math.round((g + m) * 255),
    Math.round((b + m) * 255),
  ];
}

function rgbToHex(r: number, g: number, b: number): string {
  const h = (n: number) => n.toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`.toUpperCase();
}

function hexToHsv(hex: string): Hsv | null {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  if (!m) return null;
  const int = parseInt(m[1], 16);
  const r = ((int >> 16) & 255) / 255;
  const g = ((int >> 8) & 255) / 255;
  const b = (int & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  let h = 0;
  if (d !== 0) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  const s = max === 0 ? 0 : d / max;
  return { h, s, v: max };
}

function hsvToHex(hsv: Hsv): string {
  return rgbToHex(...hsvToRgb(hsv.h, hsv.s, hsv.v));
}

export function ColorPicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (hex: string) => void;
}) {
  const [hsv, setHsv] = useState<Hsv>(() => hexToHsv(value) ?? { h: 210, s: 1, v: 1 });
  const [hexText, setHexText] = useState(value);
  const svRef = useRef<HTMLDivElement>(null);
  const hueRef = useRef<HTMLDivElement>(null);

  // 外部 value 变化（如父组件重置）时同步内部状态。
  useEffect(() => {
    const parsed = hexToHsv(value);
    if (parsed && value.toUpperCase() !== hsvToHex(hsv).toUpperCase()) {
      setHsv(parsed);
      setHexText(value.toUpperCase());
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  const emit = (next: Hsv) => {
    setHsv(next);
    const hex = hsvToHex(next);
    setHexText(hex);
    onChange(hex);
  };

  const handleSv = (clientX: number, clientY: number) => {
    const rect = svRef.current?.getBoundingClientRect();
    if (!rect) return;
    const s = clamp01((clientX - rect.left) / rect.width);
    const v = 1 - clamp01((clientY - rect.top) / rect.height);
    emit({ h: hsv.h, s, v });
  };

  const handleHue = (clientX: number) => {
    const rect = hueRef.current?.getBoundingClientRect();
    if (!rect) return;
    const h = clamp01((clientX - rect.left) / rect.width) * 360;
    emit({ h, s: hsv.s, v: hsv.v });
  };

  const onHexInput = (text: string) => {
    setHexText(text);
    const parsed = hexToHsv(text);
    if (parsed) {
      setHsv(parsed);
      onChange(hsvToHex(parsed));
    }
  };

  const hueColor = `hsl(${hsv.h}, 100%, 50%)`;
  const currentHex = hsvToHex(hsv);

  return (
    <div className="flex flex-col gap-3">
      {/* 饱和度 / 明度方块 */}
      <div
        ref={svRef}
        className="relative h-40 w-full cursor-crosshair rounded-lg"
        style={{
          background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent), ${hueColor}`,
        }}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          handleSv(e.clientX, e.clientY);
        }}
        onPointerMove={(e) => {
          if (e.buttons === 1) handleSv(e.clientX, e.clientY);
        }}
      >
        <div
          className="pointer-events-none absolute h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow"
          style={{
            left: `${hsv.s * 100}%`,
            top: `${(1 - hsv.v) * 100}%`,
            backgroundColor: currentHex,
          }}
        />
      </div>

      {/* 色相滑条 */}
      <div
        ref={hueRef}
        className="relative h-3 w-full cursor-pointer rounded-full"
        style={{
          background:
            "linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)",
        }}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          handleHue(e.clientX);
        }}
        onPointerMove={(e) => {
          if (e.buttons === 1) handleHue(e.clientX);
        }}
      >
        <div
          className="pointer-events-none absolute top-1/2 h-4 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow"
          style={{ left: `${(hsv.h / 360) * 100}%`, backgroundColor: hueColor }}
        />
      </div>

      {/* 预览块 + 十六进制输入 */}
      <div className="flex items-center gap-2">
        <span
          className="h-8 w-8 shrink-0 rounded-md border border-border"
          style={{ backgroundColor: currentHex }}
        />
        <input
          type="text"
          value={hexText}
          onChange={(e) => onHexInput(e.target.value)}
          spellCheck={false}
          className="min-w-0 flex-1 rounded-md border border-input bg-background px-3 py-1.5 text-sm text-foreground outline-none focus:border-ring"
        />
      </div>
    </div>
  );
}
