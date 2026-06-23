# Building Presentations with PptxGenJS

## Getting Started

```javascript
const PptxGenJS = require("pptxgenjs");

let deck = new PptxGenJS();
deck.layout = 'LAYOUT_16x9';  // alternatives: 'LAYOUT_16x10', 'LAYOUT_4x3', 'LAYOUT_WIDE'
deck.author = 'Your Name';
deck.title = 'Presentation Title';

let s = deck.addSlide();
s.addText("Hello World!", { x: 0.5, y: 0.5, fontSize: 36, color: "363636" });

deck.writeFile({ fileName: "Presentation.pptx" });
```

---

## Canvas Dimensions

All coordinates are specified in inches:

| Layout | Width | Height |
|--------|-------|--------|
| `LAYOUT_16x9` | 10″ | 5.625″ |
| `LAYOUT_16x10` | 10″ | 6.25″ |
| `LAYOUT_4x3` | 10″ | 7.5″ |
| `LAYOUT_WIDE` | 13.3″ | 7.5″ |

---

## Text

### Basic Text

```javascript
s.addText("Simple Text", {
  x: 1, y: 1, w: 8, h: 2, fontSize: 24, fontFace: "Arial",
  color: "363636", bold: true, align: "center", valign: "middle"
});
```

### Letter Spacing

```javascript
// charSpacing is the correct property — letterSpacing is silently ignored
s.addText("SPACED TEXT", { x: 1, y: 1, w: 8, h: 1, charSpacing: 6 });
```

### Styled Segments

```javascript
s.addText([
  { text: "Bold ", options: { bold: true } },
  { text: "Italic ", options: { italic: true } }
], { x: 1, y: 3, w: 8, h: 1 });
```

### Multi-Line Text

```javascript
// breakLine: true is mandatory between segments
s.addText([
  { text: "Line 1", options: { breakLine: true } },
  { text: "Line 2", options: { breakLine: true } },
  { text: "Line 3" }  // final segment omits breakLine
], { x: 0.5, y: 0.5, w: 8, h: 2 });
```

### Eliminating Internal Padding

```javascript
// required when text must align precisely with adjacent shapes or icons
s.addText("Title", {
  x: 0.5, y: 0.3, w: 9, h: 0.6,
  margin: 0
});
```

**Note:** Text boxes carry a default internal margin. Set `margin: 0` whenever pixel-perfect alignment with shapes, rules, or icons at identical x-coordinates is needed.

---

## Lists

### Bullet Points

```javascript
// Multiple bullet points (correct approach)
s.addText([
  { text: "First item", options: { bullet: true, breakLine: true } },
  { text: "Second item", options: { bullet: true, breakLine: true } },
  { text: "Third item", options: { bullet: true } }
], { x: 0.5, y: 0.5, w: 8, h: 3 });

// WRONG: unicode bullets produce duplicates
s.addText("• First item", { ... });  // results in double bullet marks
```

### Indentation and Numbering

```javascript
{ text: "Sub-item", options: { bullet: true, indentLevel: 1 } }
{ text: "First", options: { bullet: { type: "number" }, breakLine: true } }
```

---

## Shapes

### Basic Shapes

```javascript
s.addShape(deck.shapes.RECTANGLE, {
  x: 0.5, y: 0.8, w: 1.5, h: 3.0,
  fill: { color: "FF0000" }, line: { color: "000000", width: 2 }
});

s.addShape(deck.shapes.OVAL, { x: 4, y: 1, w: 2, h: 2, fill: { color: "0000FF" } });

s.addShape(deck.shapes.LINE, {
  x: 1, y: 3, w: 5, h: 0, line: { color: "FF0000", width: 3, dashType: "dash" }
});
```

### Transparency

```javascript
s.addShape(deck.shapes.RECTANGLE, {
  x: 1, y: 1, w: 3, h: 2,
  fill: { color: "0088CC", transparency: 50 }
});
```

### Rounded Corners

```javascript
// ROUNDED_RECTANGLE only — rectRadius has no effect on RECTANGLE
// Caution: rectangular accent overlays will not cover rounded corners. Prefer RECTANGLE when using accent bars.
s.addShape(deck.shapes.ROUNDED_RECTANGLE, {
  x: 1, y: 1, w: 3, h: 2,
  fill: { color: "FFFFFF" }, rectRadius: 0.1
});
```

### Drop Shadow

```javascript
s.addShape(deck.shapes.RECTANGLE, {
  x: 1, y: 1, w: 3, h: 2,
  fill: { color: "FFFFFF" },
  shadow: { type: "outer", color: "000000", blur: 6, offset: 2, angle: 135, opacity: 0.15 }
});
```

### Shadow Configuration Reference

| Field | Type | Values | Remarks |
|-------|------|--------|---------|
| `type` | string | `"outer"`, `"inner"` | |
| `color` | string | 6-digit hex (`"000000"`) | Omit `#`; never use 8-digit hex — see pitfalls below |
| `blur` | number | 0–100 pt | |
| `offset` | number | 0–200 pt | **Negative values corrupt the output file** |
| `angle` | number | 0–359° | Shadow direction (135 = lower-right, 270 = upward) |
| `opacity` | number | 0.0–1.0 | Always use this for transparency — never embed alpha in the color string |

For an upward shadow (e.g., above a footer bar), set `angle: 270` with a positive offset. Do **not** use negative offset values.

**Gradient fills are unsupported natively.** Use a pre-rendered gradient image as a slide background instead.

---

## Images

### Source Types

```javascript
// Local file
s.addImage({ path: "images/chart.png", x: 1, y: 1, w: 5, h: 3 });

// Remote URL
s.addImage({ path: "https://example.com/image.jpg", x: 1, y: 1, w: 5, h: 3 });

// Inline base64 (avoids filesystem I/O)
s.addImage({ data: "image/png;base64,iVBORw0KGgo...", x: 1, y: 1, w: 5, h: 3 });
```

### Additional Properties

```javascript
s.addImage({
  path: "image.png",
  x: 1, y: 1, w: 5, h: 3,
  rotate: 45,              // degrees (0–359)
  rounding: true,          // circular mask
  transparency: 50,        // 0–100
  flipH: true,             // mirror horizontally
  flipV: false,            // mirror vertically
  altText: "Description",  // accessibility label
  hyperlink: { url: "https://example.com" }
});
```

### Scaling Modes

```javascript
// Fit within bounds (preserves ratio)
{ sizing: { type: 'contain', w: 4, h: 3 } }

// Fill bounds completely (preserves ratio, may clip)
{ sizing: { type: 'cover', w: 4, h: 3 } }

// Extract a region
{ sizing: { type: 'crop', x: 0.5, y: 0.5, w: 2, h: 2 } }
```

### Maintaining Aspect Ratio

```javascript
const srcW = 1978, srcH = 923, targetH = 3.0;
const scaledW = targetH * (srcW / srcH);
const offsetX = (10 - scaledW) / 2;

s.addImage({ path: "image.png", x: offsetX, y: 1.2, w: scaledW, h: targetH });
```

### Accepted Formats

- **Raster**: PNG, JPG, GIF (animated GIF playback requires Microsoft 365)
- **Vector**: SVG (supported by modern PowerPoint / Microsoft 365)

---

## Icons

Render icons from react-icons as SVG, then rasterize to PNG for broad compatibility.

### Preparation

```javascript
const React = require("react");
const ReactDOMServer = require("react-dom/server");
const sharp = require("sharp");
const { FaCheckCircle, FaChartLine } = require("react-icons/fa");

function buildSvgMarkup(Component, hexColor = "#000000", px = 256) {
  return ReactDOMServer.renderToStaticMarkup(
    React.createElement(Component, { color: hexColor, size: String(px) })
  );
}

async function rasterizeIcon(Component, hexColor, px = 256) {
  const markup = buildSvgMarkup(Component, hexColor, px);
  const buf = await sharp(Buffer.from(markup)).png().toBuffer();
  return "image/png;base64," + buf.toString("base64");
}
```

### Placing an Icon on a Slide

```javascript
const b64 = await rasterizeIcon(FaCheckCircle, "#4472C4", 256);

s.addImage({
  data: b64,
  x: 1, y: 1, w: 0.5, h: 0.5  // display size in inches
});
```

**Tip:** Rasterize at 256px or larger for crisp results. The pixel parameter governs rendering resolution, not the on-slide dimensions (controlled by `w` and `h`).

### Icon Packages

Install globally: `npm install -g react-icons react react-dom sharp`

Commonly used sets within react-icons:
- `react-icons/fa` — Font Awesome
- `react-icons/md` — Material Design
- `react-icons/hi` — Heroicons
- `react-icons/bi` — Bootstrap Icons

---

## Backgrounds

```javascript
// Flat color
s.background = { color: "F1F1F1" };

// Transparent color
s.background = { color: "FF3399", transparency: 50 };

// Remote image
s.background = { path: "https://example.com/bg.jpg" };

// Inline base64 image
s.background = { data: "image/png;base64,iVBORw0KGgo..." };
```

---

## Tables

```javascript
s.addTable([
  ["Header 1", "Header 2"],
  ["Cell 1", "Cell 2"]
], {
  x: 1, y: 1, w: 8, h: 2,
  border: { pt: 1, color: "999999" }, fill: { color: "F1F1F1" }
});

// Styled cells with column spanning
let rows = [
  [{ text: "Header", options: { fill: { color: "6699CC" }, color: "FFFFFF", bold: true } }, "Cell"],
  [{ text: "Merged", options: { colspan: 2 } }]
];
s.addTable(rows, { x: 1, y: 3.5, w: 8, colW: [4, 4] });
```

---

## Charts

### Bar Chart

```javascript
s.addChart(deck.charts.BAR, [{
  name: "Sales", labels: ["Q1", "Q2", "Q3", "Q4"], values: [4500, 5500, 6200, 7100]
}], {
  x: 0.5, y: 0.6, w: 6, h: 3, barDir: 'col',
  showTitle: true, title: 'Quarterly Sales'
});
```

### Line Chart

```javascript
s.addChart(deck.charts.LINE, [{
  name: "Temp", labels: ["Jan", "Feb", "Mar"], values: [32, 35, 42]
}], { x: 0.5, y: 4, w: 6, h: 3, lineSize: 3, lineSmooth: true });
```

### Pie Chart

```javascript
s.addChart(deck.charts.PIE, [{
  name: "Share", labels: ["A", "B", "Other"], values: [35, 45, 20]
}], { x: 7, y: 1, w: 5, h: 4, showPercent: true });
```

### Polishing Chart Appearance

Out-of-the-box charts look outdated. These options produce a contemporary, clean style:

```javascript
s.addChart(deck.charts.BAR, chartData, {
  x: 0.5, y: 1, w: 9, h: 4, barDir: "col",

  // Palette aligned with your deck
  chartColors: ["0D9488", "14B8A6", "5EEAD4"],

  // Neutral chart area
  chartArea: { fill: { color: "FFFFFF" }, roundedCorners: true },

  // Subdued axis labels
  catAxisLabelColor: "64748B",
  valAxisLabelColor: "64748B",

  // Faint gridlines (value axis only)
  valGridLine: { color: "E2E8F0", size: 0.5 },
  catGridLine: { style: "none" },

  // Labels directly on bars
  showValue: true,
  dataLabelPosition: "outEnd",
  dataLabelColor: "1E293B",

  // Single series needs no legend
  showLegend: false,
});
```

**Essential styling properties:**
- `chartColors: [...]` — hex values for series or segments
- `chartArea: { fill, border, roundedCorners }` — chart background styling
- `catGridLine / valGridLine: { color, style, size }` — gridline control (`style: "none"` hides them)
- `lineSmooth: true` — spline curves for line charts
- `legendPos: "r"` — legend placement: "b", "t", "l", "r", "tr"

---

## Slide Masters

```javascript
deck.defineSlideMaster({
  title: 'TITLE_SLIDE', background: { color: '283A5E' },
  objects: [{
    placeholder: { options: { name: 'title', type: 'title', x: 1, y: 2, w: 8, h: 2 } }
  }]
});

let opening = deck.addSlide({ masterName: "TITLE_SLIDE" });
opening.addText("My Title", { placeholder: "title" });
```

---

## Pitfalls and Gotchas

> These mistakes lead to corrupted files, rendering glitches, or broken output. Each entry is a hard rule.

| # | Rule | Bad | Good |
|---|------|-----|------|
| 1 | Hex colors must omit `#` | `color: "#FF0000"` | `color: "FF0000"` |
| 2 | Never embed alpha in color strings | `color: "00000020"` | `color: "000000", opacity: 0.12` |
| 3 | Use `bullet: true` for bullets | `"• First item"` | `{ bullet: true }` |
| 4 | Include `breakLine: true` between segments | omitting it | `{ breakLine: true }` |
| 5 | Avoid `lineSpacing` with bullet lists | `lineSpacing` | `paraSpaceAfter` |
| 6 | Fresh instance per presentation | reusing `pptxgen()` | `new PptxGenJS()` each time |
| 7 | Never share mutable option objects | reusing same `shadow` obj | factory function per call |
| 8 | No `ROUNDED_RECTANGLE` with accent overlays | rounded + rect bar | plain `RECTANGLE` |

### Detailed Examples

**Pitfall 2 — Alpha in color strings:**
```javascript
// CORRUPTS FILE
shadow: { type: "outer", blur: 6, offset: 2, color: "00000020" }
// correct
shadow: { type: "outer", blur: 6, offset: 2, color: "000000", opacity: 0.12 }
```

**Pitfall 7 — Shared mutable objects:**
```javascript
// WRONG: second use gets corrupted values (library modifies in place)
const cfg = { type: "outer", blur: 6, offset: 2, color: "000000", opacity: 0.15 };
s.addShape(deck.shapes.RECTANGLE, { shadow: cfg, ... });
s.addShape(deck.shapes.RECTANGLE, { shadow: cfg, ... });

// CORRECT: fresh object each time
const shadowFactory = () => ({ type: "outer", blur: 6, offset: 2, color: "000000", opacity: 0.15 });
s.addShape(deck.shapes.RECTANGLE, { shadow: shadowFactory(), ... });
s.addShape(deck.shapes.RECTANGLE, { shadow: shadowFactory(), ... });
```

**Pitfall 8 — Rounded rectangle with accent bar:**
```javascript
// WRONG: accent bar leaves rounded corners exposed
s.addShape(deck.shapes.ROUNDED_RECTANGLE, { x: 1, y: 1, w: 3, h: 1.5, fill: { color: "FFFFFF" } });
s.addShape(deck.shapes.RECTANGLE, { x: 1, y: 1, w: 0.08, h: 1.5, fill: { color: "0891B2" } });

// CORRECT: flat rectangle aligns cleanly
s.addShape(deck.shapes.RECTANGLE, { x: 1, y: 1, w: 3, h: 1.5, fill: { color: "FFFFFF" } });
s.addShape(deck.shapes.RECTANGLE, { x: 1, y: 1, w: 0.08, h: 1.5, fill: { color: "0891B2" } });
```

---

## Quick Reference

| Category | Options |
|----------|---------|
| **Shape types** | RECTANGLE, OVAL, LINE, ROUNDED_RECTANGLE |
| **Chart types** | BAR, LINE, PIE, DOUGHNUT, SCATTER, BUBBLE, RADAR |
| **Page layouts** | LAYOUT_16x9 (10″×5.625″), LAYOUT_16x10, LAYOUT_4x3, LAYOUT_WIDE |
| **Text alignment** | "left", "center", "right" |
| **Data label positions** | "outEnd", "inEnd", "center" |
