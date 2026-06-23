"""Produce thumbnail grid images from PowerPoint presentations.

Renders a labeled grid of slide previews for rapid visual inspection.
Each cell shows the XML filename (e.g., slide1.xml) as a caption.
Hidden slides appear with a diagonal-cross placeholder pattern.

Invocation:
    python thumbnail.py input.pptx [prefix] [--cols N]

Samples:
    python thumbnail.py presentation.pptx
    # Writes: thumbnails.jpg

    python thumbnail.py template.pptx grid --cols 4
    # Writes: grid.jpg (or grid-1.jpg, grid-2.jpg for large decks)
"""

import argparse
import subprocess
import sys
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path

import defusedxml.minidom
from office.soffice import get_soffice_env
from PIL import Image, ImageDraw, ImageFont

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

CELL_WIDTH_PX = 300
RENDER_DPI = 100
COLUMN_CEILING = 6
COLUMN_DEFAULT = 3
JPEG_QUALITY = 95
GRID_PADDING_PX = 20
BORDER_STROKE_PX = 2
FONT_SCALE_FACTOR = 0.10
CAPTION_GAP_FACTOR = 0.4
DEFAULT_SLIDE_DIMENSIONS = (1920, 1080)

HIDDEN_SLIDE_BG_COLOR = "#F0F0F0"
HIDDEN_SLIDE_LINE_COLOR = "#CCCCCC"
CAPTION_TEXT_COLOR = "black"
GRID_BG_COLOR = "white"
BORDER_COLOR = "gray"


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass
class SlideMetadata:
    """Represents a single slide's identity and visibility state."""

    name: str
    hidden: bool


@dataclass
class GridEntry:
    """A slide image paired with its display caption."""

    image_path: Path
    caption: str


# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------


def parse_args() -> argparse.Namespace:
    """Parse and return command-line arguments."""
    ap = argparse.ArgumentParser(
        description="Produce thumbnail grids from PowerPoint slides."
    )
    ap.add_argument("input", help="Source PowerPoint file (.pptx)")
    ap.add_argument(
        "output_prefix",
        nargs="?",
        default="thumbnails",
        help="Filename prefix for output images (default: thumbnails)",
    )
    ap.add_argument(
        "--cols",
        type=int,
        default=COLUMN_DEFAULT,
        help="Column count (default: %d, max: %d)" % (COLUMN_DEFAULT, COLUMN_CEILING),
    )
    return ap.parse_args()


# ---------------------------------------------------------------------------
# Slide metadata extraction
# ---------------------------------------------------------------------------


def extract_slide_metadata(pptx_fp: Path) -> list[SlideMetadata]:
    """Read presentation.xml to obtain ordered slide names and visibility."""
    with zipfile.ZipFile(pptx_fp, "r") as zf:
        rels_raw = zf.read("ppt/_rels/presentation.xml.rels").decode("utf-8")
        rels_dom = defusedxml.minidom.parseString(rels_raw)

        id_map: dict[str, str] = {}
        for el in rels_dom.getElementsByTagName("Relationship"):
            key = el.getAttribute("Id")
            tgt = el.getAttribute("Target")
            rtype = el.getAttribute("Type")
            if "slide" in rtype and tgt.startswith("slides/"):
                id_map[key] = tgt.split("slides/")[-1]

        pres_raw = zf.read("ppt/presentation.xml").decode("utf-8")
        pres_dom = defusedxml.minidom.parseString(pres_raw)

        entries: list[SlideMetadata] = []
        for tag in pres_dom.getElementsByTagName("p:sldId"):
            r = tag.getAttribute("r:id")
            if r in id_map:
                is_hidden = tag.getAttribute("show") == "0"
                entries.append(SlideMetadata(name=id_map[r], hidden=is_hidden))

        return entries


# ---------------------------------------------------------------------------
# Image rendering
# ---------------------------------------------------------------------------


def render_pdf_then_images(pptx_fp: Path, work_dir: Path) -> list[Path]:
    """Convert PPTX → PDF → per-slide JPEG images."""
    pdf_fp = work_dir / ("%s.pdf" % pptx_fp.stem)

    proc = subprocess.run(
        ["soffice", "--headless", "--convert-to", "pdf",
         "--outdir", str(work_dir), str(pptx_fp)],
        capture_output=True, text=True, env=get_soffice_env(),
    )
    if proc.returncode != 0 or not pdf_fp.exists():
        raise RuntimeError("PDF conversion failed")

    proc = subprocess.run(
        ["pdftoppm", "-jpeg", "-r", str(RENDER_DPI),
         str(pdf_fp), str(work_dir / "slide")],
        capture_output=True, text=True,
    )
    if proc.returncode != 0:
        raise RuntimeError("Image conversion failed")

    return sorted(work_dir.glob("slide-*.jpg"))


# ---------------------------------------------------------------------------
# Placeholder generation
# ---------------------------------------------------------------------------


def make_hidden_placeholder(dims: tuple[int, int]) -> Image.Image:
    """Create a diagonal-cross pattern for hidden slides."""
    canvas = Image.new("RGB", dims, color=HIDDEN_SLIDE_BG_COLOR)
    pen = ImageDraw.Draw(canvas)
    thickness = max(5, min(dims) // 100)
    pen.line([(0, 0), dims], fill=HIDDEN_SLIDE_LINE_COLOR, width=thickness)
    pen.line([(dims[0], 0), (0, dims[1])], fill=HIDDEN_SLIDE_LINE_COLOR, width=thickness)
    return canvas


# ---------------------------------------------------------------------------
# Grid entry assembly
# ---------------------------------------------------------------------------


def assemble_grid_entries(
    metadata: list[SlideMetadata], visible_images: list[Path], scratch_dir: Path
) -> list[GridEntry]:
    """Pair each slide with its image path and caption."""
    placeholder_dims = DEFAULT_SLIDE_DIMENSIONS
    if visible_images:
        with Image.open(visible_images[0]) as ref:
            placeholder_dims = ref.size

    entries: list[GridEntry] = []
    visible_idx = 0

    for slide in metadata:
        if slide.hidden:
            ph_path = scratch_dir / ("hidden-%s.jpg" % slide.name)
            make_hidden_placeholder(placeholder_dims).save(ph_path, "JPEG")
            entries.append(GridEntry(image_path=ph_path, caption="%s (hidden)" % slide.name))
        else:
            if visible_idx < len(visible_images):
                entries.append(GridEntry(image_path=visible_images[visible_idx], caption=slide.name))
                visible_idx += 1

    return entries


# ---------------------------------------------------------------------------
# Grid composition
# ---------------------------------------------------------------------------


def compose_grid(entries: list[GridEntry], ncols: int, cell_w: int) -> Image.Image:
    """Lay out thumbnails in a labeled grid."""
    font_size = int(cell_w * FONT_SCALE_FACTOR)
    caption_padding = int(font_size * CAPTION_GAP_FACTOR)

    with Image.open(entries[0].image_path) as sample:
        aspect_ratio = sample.height / sample.width
    cell_h = int(cell_w * aspect_ratio)

    nrows = -(-len(entries) // ncols)  # ceiling division
    total_w = ncols * cell_w + (ncols + 1) * GRID_PADDING_PX
    total_h = nrows * (cell_h + font_size + caption_padding * 2) + (nrows + 1) * GRID_PADDING_PX

    canvas = Image.new("RGB", (total_w, total_h), GRID_BG_COLOR)
    pen = ImageDraw.Draw(canvas)

    try:
        fnt = ImageFont.load_default(size=font_size)
    except Exception:
        fnt = ImageFont.load_default()

    for idx, entry in enumerate(entries):
        row, col = divmod(idx, ncols)
        ox = col * cell_w + (col + 1) * GRID_PADDING_PX
        oy = row * (cell_h + font_size + caption_padding * 2) + (row + 1) * GRID_PADDING_PX

        # Draw caption centered above the thumbnail
        bbox = pen.textbbox((0, 0), entry.caption, font=fnt)
        text_width = bbox[2] - bbox[0]
        pen.text(
            (ox + (cell_w - text_width) // 2, oy + caption_padding),
            entry.caption, fill=CAPTION_TEXT_COLOR, font=fnt,
        )

        thumb_y = oy + caption_padding + font_size + caption_padding

        # Paste the thumbnail image
        with Image.open(entry.image_path) as im:
            im.thumbnail((cell_w, cell_h), Image.Resampling.LANCZOS)
            iw, ih = im.size
            tx = ox + (cell_w - iw) // 2
            ty = thumb_y + (cell_h - ih) // 2
            canvas.paste(im, (tx, ty))

            if BORDER_STROKE_PX > 0:
                pen.rectangle(
                    [(tx - BORDER_STROKE_PX, ty - BORDER_STROKE_PX),
                     (tx + iw + BORDER_STROKE_PX - 1, ty + ih + BORDER_STROKE_PX - 1)],
                    outline=BORDER_COLOR, width=BORDER_STROKE_PX,
                )

    return canvas


# ---------------------------------------------------------------------------
# Grid output
# ---------------------------------------------------------------------------


def write_grids(
    entries: list[GridEntry], ncols: int, cell_w: int, out_fp: Path
) -> list[str]:
    """Split entries into pages and write grid images."""
    page_capacity = ncols * (ncols + 1)
    written: list[str] = []

    for chunk_idx, start in enumerate(range(0, len(entries), page_capacity)):
        chunk = entries[start:min(start + page_capacity, len(entries))]
        grid = compose_grid(chunk, ncols, cell_w)

        if len(entries) <= page_capacity:
            dest = out_fp
        else:
            dest = out_fp.parent / ("%s-%d%s" % (out_fp.stem, chunk_idx + 1, out_fp.suffix))

        dest.parent.mkdir(parents=True, exist_ok=True)
        grid.save(str(dest), quality=JPEG_QUALITY)
        written.append(str(dest))

    return written


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------


def main() -> None:
    """Parse arguments and generate thumbnail grids."""
    args = parse_args()

    ncols = min(args.cols, COLUMN_CEILING)
    if args.cols > COLUMN_CEILING:
        print("Warning: Columns limited to %d" % COLUMN_CEILING)

    src = Path(args.input)
    if not src.exists() or src.suffix.lower() != ".pptx":
        print("Error: Invalid PowerPoint file: %s" % args.input, file=sys.stderr)
        sys.exit(1)

    dest = Path("%s.jpg" % args.output_prefix)

    try:
        metadata = extract_slide_metadata(src)

        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            vis_imgs = render_pdf_then_images(src, td_path)

            if not vis_imgs and not any(s.hidden for s in metadata):
                print("Error: No slides found", file=sys.stderr)
                sys.exit(1)

            entries = assemble_grid_entries(metadata, vis_imgs, td_path)
            outputs = write_grids(entries, ncols, CELL_WIDTH_PX, dest)

            print("Created %d grid(s):" % len(outputs))
            for o in outputs:
                print("  %s" % o)

    except Exception as exc:
        print("Error: %s" % exc, file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
