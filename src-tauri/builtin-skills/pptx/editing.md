# Modifying Existing Presentations

## Workflow Overview

```
┌─────────────┐    ┌──────────┐    ┌────────────┐    ┌──────────┐    ┌─────────┐    ┌─────────┐
│ 1. Examine  │───▶│ 2. Plan  │───▶│ 3. Unpack  │───▶│ 4. Edit  │───▶│ 5. Clean│───▶│ 6. Pack │
│  template   │    │  layout  │    │            │    │  slides  │    │         │    │         │
└─────────────┘    └──────────┘    └────────────┘    └──────────┘    └─────────┘    └─────────┘
```

---

## Step 1 — Examine the Source Deck

```bash
python scripts/thumbnail.py template.pptx
python -m markitdown template.pptx
```

Open `thumbnails.jpg` to survey available layouts; review markitdown output for placeholder text.

## Step 2 — Map Content to Slide Layouts

Assign each content section to a suitable template slide.

**VARIETY IS ESSENTIAL** — uniformity kills engagement. Resist the urge to default to title-plus-bullets for every slide. Actively look for:
- Two-column or three-column arrangements
- Visual-plus-text composites
- Edge-to-edge imagery with overlay text
- Standalone quote or highlight slides
- Section break cards
- Large-number callout panels
- Icon-grid or icon-row layouts

**Stay away from** repeating the same text-heavy template across the deck.

Align layout style with content type (bullet points → list layout, personnel bios → multi-column, praise → quote slide).

## Step 3 — Extract

```bash
python scripts/office/unpack.py template.pptx unpacked/
```

## Step 4 — Reorganize the Deck

> Handle this personally, not via subagents.

- Discard unwanted slides (strip their entries from `<p:sldIdLst>`)
- Clone slides needed more than once (`add_slide.py`)
- Rearrange `<p:sldId>` entries for desired order
- **Finish all structural modifications before moving to step 5**

## Step 5 — Populate Content

Replace placeholder text within each `slide{N}.xml`.

**Subagents are ideal here** — because slides live in separate XML files, parallel editing is safe.

## Step 6 — Finalize

```bash
python scripts/clean.py unpacked/
python scripts/office/pack.py unpacked/ output.pptx --original template.pptx
```

---

## Available Utilities

| Utility | Purpose |
|---------|---------|
| `unpack.py` | Decompresses and formats PPTX XML for editing |
| `add_slide.py` | Clones an existing slide or instantiates one from a layout |
| `clean.py` | Deletes orphaned assets |
| `pack.py` | Repackages with integrity checks |
| `thumbnail.py` | Generates a visual slide overview grid |

### unpack.py

```bash
python scripts/office/unpack.py input.pptx unpacked/
```

Decompresses the archive, reformats XML for readability, and normalizes curly quotes.

### add_slide.py

```bash
python scripts/add_slide.py unpacked/ slide2.xml      # clone a slide
python scripts/add_slide.py unpacked/ slideLayout2.xml # instantiate from layout
```

Outputs the `<p:sldId>` element you should insert into `<p:sldIdLst>` at the desired position.

### clean.py

```bash
python scripts/clean.py unpacked/
```

Purges slides absent from `<p:sldIdLst>`, dangling media assets, and stale relationship entries.

### pack.py

```bash
python scripts/office/pack.py unpacked/ output.pptx --original input.pptx
```

Runs validation and auto-repair, compresses XML whitespace, and restores curly quote encoding.

### thumbnail.py

```bash
python scripts/thumbnail.py input.pptx [output_prefix] [--cols N]
```

Produces `thumbnails.jpg` — a labeled grid showing each slide's filename (e.g., slide1.xml). Defaults to 3 columns, capped at 12 slides per grid image.

**Intended for template selection only** (picking which layout to reuse). For accurate visual QA, render full-resolution individual images via `soffice` + `pdftoppm` — described in SKILL.md.

---

## Manipulating Slides

The slide sequence is stored in `ppt/presentation.xml` under `<p:sldIdLst>`.

| Operation | Procedure |
|-----------|-----------|
| **Reorder** | Move `<p:sldId>` elements to reflect the new sequence |
| **Remove** | Delete the corresponding `<p:sldId>`, then execute `clean.py` |
| **Insert** | Always use `add_slide.py` — manual duplication is error-prone (notes refs, Content_Types, relationship IDs) |

---

## Replacing Content

**Subagent delegation:** When subagents are available, assign slide editing after step 4 is complete. Each slide resides in its own XML file, enabling parallel work. Provide subagents with:
- The target slide file path(s)
- **"Use the Edit tool for all modifications"**
- The formatting conventions and pitfall list below

Per-slide process:
1. Read the slide XML
2. Locate every placeholder — text, images, charts, icons, captions
3. Swap each placeholder with the final content

**Always use the Edit tool, never sed or programmatic scripts.** The Edit tool enforces precision about what gets replaced and where, which produces more reliable outcomes.

### Formatting Conventions

- **Apply bold to all headings, subheadings, and inline labels**: Set `b="1"` on `<a:rPr>` for:
  - Slide titles
  - In-slide section headings
  - Leading labels within a line (e.g., "Status:", "Description:")
- **Avoid unicode bullet characters (•)**: Employ proper OOXML list markup via `<a:buChar>` or `<a:buAutoNum>`
- **Preserve bullet inheritance**: Let bullets derive from the layout. Only override with `<a:buChar>` or `<a:buNone>`.

---

## Frequent Mistakes

### Adapting Template Slots

When source material has fewer items than the template provides:
- **Delete surplus elements entirely** (graphics, shapes, text boxes) — do not merely blank out text
- Watch for orphaned visuals after clearing textual content
- Confirm via visual QA that element counts match

When replacement text differs in length:
- **Shorter text**: Generally safe
- **Longer text**: Risk of overflow or unexpected wrapping
- Verify via visual QA after any substitution
- If needed, abbreviate or redistribute content to respect the template's spatial constraints

**Template capacity ≠ source content count**: If a template provides 4 team-member slots but the source lists 3, remove the 4th slot's entire shape group (photo + text boxes), not just the text.

### Multi-Entry Content

When source data contains enumerated items or multiple paragraphs, generate individual `<a:p>` elements — **never concatenate everything into a single text node**.

**Wrong approach** — everything in one paragraph:
```xml
<a:p>
  <a:r><a:rPr .../><a:t>Step 1: Do the first thing. Step 2: Do the second thing.</a:t></a:r>
</a:p>
```

**Correct approach** — discrete paragraphs with bolded headings:
```xml
<a:p>
  <a:pPr algn="l"><a:lnSpc><a:spcPts val="3919"/></a:lnSpc></a:pPr>
  <a:r><a:rPr lang="en-US" sz="2799" b="1" .../><a:t>Step 1</a:t></a:r>
</a:p>
<a:p>
  <a:pPr algn="l"><a:lnSpc><a:spcPts val="3919"/></a:lnSpc></a:pPr>
  <a:r><a:rPr lang="en-US" sz="2799" .../><a:t>Do the first thing.</a:t></a:r>
</a:p>
<a:p>
  <a:pPr algn="l"><a:lnSpc><a:spcPts val="3919"/></a:lnSpc></a:pPr>
  <a:r><a:rPr lang="en-US" sz="2799" b="1" .../><a:t>Step 2</a:t></a:r>
</a:p>
<!-- repeat as needed -->
```

Replicate `<a:pPr>` from the source paragraph to maintain consistent line spacing. Apply `b="1"` to heading runs.

### Curly Quote Handling

The unpack/pack pipeline handles smart quote conversion automatically. However, the Edit tool normalizes curly quotes to ASCII.

**When inserting new text that contains quotation marks, use XML character references:**

```xml
<a:t>the &#x201C;Agreement&#x201D;</a:t>
```

| Glyph | Description | Code Point | XML Reference |
|-------|-------------|-----------|---------------|
| \u201c | Opening double quote | U+201C | `&#x201C;` |
| \u201d | Closing double quote | U+201D | `&#x201D;` |
| \u2018 | Opening single quote | U+2018 | `&#x2018;` |
| \u2019 | Closing single quote | U+2019 | `&#x2019;` |

### Additional Notes

- **Leading/trailing whitespace**: Attach `xml:space="preserve"` to any `<a:t>` element containing spaces at the start or end
- **XML manipulation library**: Use `defusedxml.minidom` exclusively — `xml.etree.ElementTree` corrupts namespace declarations
