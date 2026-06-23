---
name: pptx
version: 1.0.1
description: "Use this skill any time a .pptx file is involved in any way — as input, output, or both. This includes: creating slide decks, pitch decks, or presentations; reading, parsing, or extracting text from any .pptx file (even if the extracted content will be used elsewhere, like in an email or summary); editing, modifying, or updating existing presentations; combining or splitting slide files; working with templates, layouts, speaker notes, or comments. Trigger whenever the user mentions \"deck,\" \"slides,\" \"presentation,\" or references a .pptx filename, regardless of what they plan to do with the content afterward. If a .pptx file needs to be opened, created, or touched, use this skill."
description_zh: "当 .pptx 文件以任何方式涉及时使用此技能——无论是作为输入、输出还是两者兼有。包括：创建幻灯片、演示文稿或路演材料；读取、解析或提取任何 .pptx 文件中的文本（即使提取的内容将用于其他地方，如邮件或摘要）；编辑、修改或更新现有演示文稿；合并或拆分幻灯片文件；使用模板、布局、演讲者备注或批注。当用户提及\"幻灯片\"、\"演示文稿\"、\"PPT\"或引用 .pptx 文件名时触发，无论他们计划如何使用内容。只要需要打开、创建或操作 .pptx 文件，就使用此技能。"
license: Proprietary. LICENSE.txt has complete terms
---

# PowerPoint Presentation Skill

> **Missing a dependency?** This skill runs external tools (e.g. `markitdown`, `Pillow`, LibreOffice/`soffice`). If a command or script fails because something isn't installed, use the **install-skill-dependency** skill (load it via `load_skill`) to detect and install the missing pieces with the user's authorization — don't run system-wide installs silently.

## At a Glance

> Quickly determine which workflow applies to your situation:

- **Extracting or reading content** — `python -m markitdown presentation.pptx`
- **Modifying an existing deck or building from a template** — see [editing.md](editing.md)
- **Generating a brand-new presentation without any template** — see [pptxgenjs.md](pptxgenjs.md)

---

## Extracting Content

Three approaches for pulling information out of a `.pptx` file:

**Plain-text extraction:**
```bash
python -m markitdown presentation.pptx
```

**Slide thumbnail overview:**
```bash
python scripts/thumbnail.py presentation.pptx
```

**Underlying XML inspection:**
```bash
python scripts/office/unpack.py presentation.pptx unpacked/
```

---

## Modifying Existing Decks

Full instructions live in [editing.md](editing.md). The condensed version:

1. Examine the template via `thumbnail.py`
2. Unpack the file, restructure slides, update content, run cleanup, then repack

---

## Building from Scratch

Full instructions live in [pptxgenjs.md](pptxgenjs.md).

Choose this path when there is no existing file or reference deck to work from.

---

## Visual Design Principles

**Boring slides are unacceptable.** White backgrounds with plain bullet points produce forgettable presentations. Apply the following guidance to every slide you build.

### Color Strategy

| Decision | Guidance |
|----------|----------|
| **Palette selection** | Colors must speak to the subject matter. If they could transfer to an unrelated topic unchanged, they are not distinctive enough. |
| **Weight hierarchy** | One dominant hue (60–70%), one or two supporting tones, one high-contrast accent. Equal distribution looks amateur. |
| **Background rhythm** | Alternate light and dark backgrounds — dark bookends (opener + closer) with lighter body slides, or fully dark for premium aesthetics. |
| **Visual motif** | Commit to one recurring treatment (circular image masks, colored icon badges, heavy one-sided borders) carried through every slide. |

### Suggested Color Palettes

Pick hues appropriate to the topic. Generic blue is a last resort.

| Name | Primary | Secondary | Accent |
|------|---------|-----------|--------|
| **Deep Night** | `1E2761` (navy) | `CADCFC` (ice blue) | `FFFFFF` (white) |
| **Woodland** | `2C5F2D` (forest) | `97BC62` (moss) | `F5F5F5` (cream) |
| **Vibrant Coral** | `F96167` (coral) | `F9E795` (gold) | `2F3C7E` (navy) |
| **Earth Tone** | `B85042` (terracotta) | `E7E8D1` (sand) | `A7BEAE` (sage) |
| **Deep Sea** | `065A82` (deep blue) | `1C7293` (teal) | `21295C` (midnight) |
| **Graphite** | `36454F` (charcoal) | `F2F2F2` (off-white) | `212121` (black) |
| **Aquatic Trust** | `028090` (teal) | `00A896` (seafoam) | `02C39A` (mint) |
| **Wine & Linen** | `6D2E46` (berry) | `A26769` (dusty rose) | `ECE2D0` (cream) |
| **Eucalyptus** | `84B59F` (sage) | `69A297` (eucalyptus) | `50808E` (slate) |
| **Crimson Impact** | `990011` (cherry) | `FCF6F5` (off-white) | `2F3C7E` (navy) |

### Layout & Composition

**No slide should exist without a visual component** — whether that is an image, chart, icon, or decorative shape.

**Arrangement patterns:**
- Split layout — text on one side, visual on the other
- Icon rows — colored circular badge + bold label + descriptive copy
- Grid blocks — 2×2 or 2×3 cards with image and text pairs
- Full-bleed imagery — half the slide is a photo, the other half is overlaid content

**Data presentation:**
- Oversized numeric callouts (60–72pt stat with a small descriptor underneath)
- Side-by-side comparison (before/after, advantages/disadvantages)
- Sequential flow (numbered steps connected by arrows)

**Finishing touches:**
- Small colored circles with icons beside section headings
- Italicized accent phrases for key takeaways or taglines

### Typography

**Avoid defaulting to Arial.** Select a header typeface with character, paired with a clean body font.

| Heading | Body |
|---------|------|
| Georgia | Calibri |
| Arial Black | Arial |
| Calibri | Calibri Light |
| Cambria | Calibri |
| Trebuchet MS | Calibri |
| Impact | Arial |
| Palatino | Garamond |
| Consolas | Calibri |

**Sizing scale:**

| Role | Size |
|------|------|
| Main slide heading | 36–44pt bold |
| Subheading | 20–24pt bold |
| Paragraph text | 14–16pt |
| Footnotes/captions | 10–12pt muted |

### Whitespace

- Maintain at least 0.5″ margins on all edges
- Allow 0.3–0.5″ gaps between content regions
- Resist filling every available inch — breathing room improves readability

### Common Mistakes

> Treat each item as a hard constraint. Violating any one degrades the final result.

| Mistake | Why it fails | Fix |
|---------|-------------|-----|
| Monotonous layouts | Viewer fatigue | Alternate between columns, card grids, and callout styles |
| Centered body copy | Reduces readability | Left-align running text; reserve centering for titles only |
| Weak size contrast | Hierarchy collapses | Titles must be 36pt+ vs 14–16pt body |
| Automatic blue | Generic appearance | Choose palette colors that suit the specific topic |
| Inconsistent spacing | Visual noise | Pick 0.3″ or 0.5″ gaps and apply uniformly |
| Partial styling | Looks unfinished | Commit fully to a visual system or keep it minimal throughout |
| Walls of text | Unreadable | Supplement every slide with imagery, icons, charts, or shapes |
| Ignoring text-box padding | Misalignment | Set `margin: 0` or offset to compensate for default padding |
| Poor contrast | Illegible content | Ensure strong contrast — light-on-light or dark-on-dark is unreadable |
| Accent lines beneath titles | Machine-generated look | Prefer whitespace or background color separation |

---

## Quality Assurance (Mandatory)

**Start with the assumption that something is wrong. Your task is to identify it.**

First drafts are virtually never error-free. Treat QA as a bug-hunting exercise rather than a rubber stamp. Finding zero issues on the first pass means you have not looked carefully enough.

### Textual QA

```bash
python -m markitdown output.pptx
```

Verify completeness, correctness, and ordering of all slide content.

**When working from templates, scan for residual placeholder text:**

```bash
python -m markitdown output.pptx | grep -iE "xxxx|lorem|ipsum|this.*(page|slide).*layout"
```

Any matches must be resolved before the presentation is considered done.

### Visual QA

**Delegate to subagents** — even for decks with only 2–3 slides. After spending time deep in the code, you will see what you expect rather than what actually rendered. Fresh eyes catch more.

Render slides to images (see [Rendering to Images](#rendering-to-images)), then provide subagents with this prompt:

```
Examine these slide images carefully. Work under the assumption that problems exist — your job is to locate them.

Check for:
- Overlapping content (text bleeding through shapes, lines crossing words, stacked items)
- Truncated or overflowing text at edges or container boundaries
- Decorative elements sized for single-line headings that now wrap to two lines
- Source attributions or footers colliding with body content
- Cramped spacing (< 0.3" between elements) or unbalanced whitespace
- Content pushed too close to slide edges (< 0.5" margin)
- Misaligned columns or parallel elements
- Hard-to-read text (light text on pale backgrounds, etc.)
- Hard-to-see icons (dark icons against dark backgrounds without contrasting circles)
- Overly narrow text boxes causing excessive line wrapping
- Unreplaced placeholder content

For every slide, enumerate any concerns — even minor ones.

Inspect the following images:
1. /path/to/slide-01.jpg (Expected: [brief description])
2. /path/to/slide-02.jpg (Expected: [brief description])

Report everything you find.
```

### Iterative Verification

1. Generate the deck → render images → inspect
2. **Document every issue found** (if the list is empty, look harder)
3. Apply fixes
4. **Re-check the affected slides** — one correction frequently introduces another
5. Loop until a complete pass surfaces no new problems

**Do not declare the work finished until at least one fix-then-verify cycle has completed.**

---

## Rendering to Images

Convert the final `.pptx` into per-slide images for visual review:

```bash
python scripts/office/soffice.py --headless --convert-to pdf output.pptx
pdftoppm -jpeg -r 150 output.pdf slide
```

This produces `slide-01.jpg`, `slide-02.jpg`, and so on.

To re-render only specific slides after corrections:

```bash
pdftoppm -jpeg -r 150 -f N -l N output.pdf slide-fixed
```

---

## Required Packages

- `pip install "markitdown[pptx]"` — text extraction
- `pip install Pillow` — thumbnail generation
- `npm install -g pptxgenjs` — scratch presentation creation
- LibreOffice (`soffice`) — PDF conversion (sandboxed environments handled automatically by `scripts/office/soffice.py`)
- Poppler (`pdftoppm`) — PDF-to-image conversion
