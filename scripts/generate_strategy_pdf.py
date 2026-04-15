#!/usr/bin/env python3
"""
Generate a professional PDF from the AMOS strategy markdown document.
Uses reportlab for advanced layout control.
"""

import re
import sys
from pathlib import Path
from reportlab.lib.pagesizes import letter
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.units import inch
from reportlab.lib.colors import HexColor, white
from reportlab.platypus import (
    SimpleDocTemplate, Paragraph, Spacer, PageBreak, Table, TableStyle,
    Preformatted
)


# Color palette
NAVY = HexColor("#1a1a2e")
TEAL = HexColor("#0d9488")
GRAY = HexColor("#666666")
LIGHT_GRAY = HexColor("#f3f4f6")
DARK_GRAY = HexColor("#374151")

PAGE_WIDTH, PAGE_HEIGHT = letter
CONTENT_WIDTH = PAGE_WIDTH - 1.5 * inch  # margins on each side


class PDFHeaderFooter:
    """Custom header and footer for each page"""
    def on_page(self, canvas, doc):
        canvas.saveState()
        width, height = letter

        # Header line (teal)
        canvas.setStrokeColor(TEAL)
        canvas.setLineWidth(1.5)
        canvas.line(0.75*inch, height - 0.6*inch, width - 0.75*inch, height - 0.6*inch)

        # Header text
        canvas.setFont("Helvetica", 9)
        canvas.setFillColor(GRAY)
        canvas.drawString(0.75*inch, height - 0.45*inch, "AMOS Labs — Strategic Overview   April 2026")

        # Footer line (teal)
        canvas.line(0.75*inch, 0.65*inch, width - 0.75*inch, 0.65*inch)

        # Page number
        canvas.setFont("Helvetica", 9)
        canvas.setFillColor(GRAY)
        canvas.drawRightString(width - 0.75*inch, 0.45*inch, f"Page {doc.page}")

        canvas.restoreState()


def create_title_page(story):
    """Create the title page"""
    story.append(Spacer(1, 2.0 * inch))

    # Subtitle line (teal)
    subtitle_style = ParagraphStyle(
        'Subtitle',
        fontName='Helvetica-Bold',
        fontSize=22,
        textColor=TEAL,
        alignment=1,
        spaceAfter=30,
        leading=28,
    )
    story.append(Paragraph("The Operating System for Autonomous Commerce", subtitle_style))

    story.append(Spacer(1, 0.5 * inch))

    # Tagline
    tagline_style = ParagraphStyle(
        'Tagline',
        fontName='Helvetica',
        fontSize=12,
        textColor=DARK_GRAY,
        alignment=1,
        spaceAfter=30,
        leading=16,
    )
    story.append(Paragraph(
        "A Strategic Thesis on Autonomous Economic Participation,<br/>the Macro Landscape, and Why AMOS Matters Now",
        tagline_style
    ))

    story.append(Spacer(1, 2.0 * inch))

    # Footer info
    footer_style = ParagraphStyle(
        'TitleFooter',
        fontName='Helvetica',
        fontSize=11,
        textColor=GRAY,
        alignment=1,
    )
    story.append(Paragraph("AMOS Labs · April 2026", footer_style))
    story.append(PageBreak())


def create_toc(story, toc_entries):
    """Create table of contents"""
    toc_title_style = ParagraphStyle(
        'TOCTitle',
        fontName='Helvetica-Bold',
        fontSize=18,
        textColor=NAVY,
        spaceAfter=24,
    )
    story.append(Paragraph("Table of Contents", toc_title_style))

    toc_h1_style = ParagraphStyle(
        'TOCH1',
        fontName='Helvetica-Bold',
        fontSize=11,
        textColor=DARK_GRAY,
        spaceAfter=8,
        leftIndent=10,
    )

    toc_h2_style = ParagraphStyle(
        'TOCH2',
        fontName='Helvetica',
        fontSize=10,
        textColor=GRAY,
        spaceAfter=6,
        leftIndent=30,
    )

    for level, entry in toc_entries:
        if level == 1:
            story.append(Paragraph(entry, toc_h1_style))
        else:
            story.append(Paragraph(entry, toc_h2_style))

    story.append(PageBreak())


def format_inline_text(text):
    """Format inline text: **bold** -> <b>bold</b>, *italic* -> <i>italic</i>"""
    # Escape XML entities first
    text = text.replace('&', '&amp;')
    text = text.replace('<', '&lt;').replace('>', '&gt;')
    # Now apply markdown formatting (after escaping, so we need to use raw tags)
    # Actually, we need bold/italic tags to work, so let's be smarter:
    # First, protect markdown bold/italic, then escape, then restore
    # Simpler approach: escape only bare < > that aren't part of our formatting
    # Let me redo this properly:
    text = text.replace('&amp;', '&')  # undo
    text = text.replace('&lt;', '<').replace('&gt;', '>')  # undo

    # Handle **bold** -> <b>bold</b>
    text = re.sub(r'\*\*([^*]+)\*\*', r'<b>\1</b>', text)
    # Handle *italic* -> <i>italic</i>
    text = re.sub(r'\*([^*]+)\*', r'<i>\1</i>', text)
    # Handle [text](url) -> just text (PDF can't click)
    text = re.sub(r'\[([^\]]+)\]\([^)]+\)', r'\1', text)

    # Escape any remaining bare & that aren't part of XML entities
    text = re.sub(r'&(?!amp;|lt;|gt;|quot;|apos;|#)', '&amp;', text)

    return text


def parse_table(lines):
    """Parse markdown table into data structure, properly handling header + separator + body"""
    rows = []
    for i, line in enumerate(lines):
        cells = [cell.strip() for cell in line.split('|')[1:-1]]
        if not cells:
            continue
        # Skip separator row (the |---|---| line)
        if all(re.match(r'^[-:]+$', c) for c in cells):
            continue
        rows.append(cells)
    return rows


def create_styled_table(data):
    """Create a styled table with teal header, wrapping text in Paragraph cells"""
    if not data or len(data) < 2:
        return None

    # Determine column count
    num_cols = len(data[0])

    # Calculate column widths proportionally
    if num_cols == 2:
        col_widths = [2.0 * inch, CONTENT_WIDTH - 2.0 * inch]
    elif num_cols == 3:
        col_widths = [CONTENT_WIDTH / 3] * 3
    else:
        col_widths = [CONTENT_WIDTH / num_cols] * num_cols

    # Header style
    header_para_style = ParagraphStyle(
        'TableHeader',
        fontName='Helvetica-Bold',
        fontSize=10,
        textColor=white,
        leading=13,
    )

    # Body cell style
    cell_para_style = ParagraphStyle(
        'TableCell',
        fontName='Helvetica',
        fontSize=9,
        textColor=DARK_GRAY,
        leading=12,
    )

    # Bold cell style (for first column in key-value tables)
    cell_bold_style = ParagraphStyle(
        'TableCellBold',
        fontName='Helvetica-Bold',
        fontSize=9,
        textColor=DARK_GRAY,
        leading=12,
    )

    # Wrap all cells in Paragraphs so text wraps properly
    wrapped_data = []
    for row_idx, row in enumerate(data):
        wrapped_row = []
        for col_idx, cell in enumerate(row):
            cell_text = format_inline_text(cell)
            if row_idx == 0:
                wrapped_row.append(Paragraph(cell_text, header_para_style))
            elif col_idx == 0 and num_cols == 2:
                wrapped_row.append(Paragraph(cell_text, cell_bold_style))
            else:
                wrapped_row.append(Paragraph(cell_text, cell_para_style))
        wrapped_data.append(wrapped_row)

    table = Table(wrapped_data, colWidths=col_widths)

    style = TableStyle([
        # Header row
        ('BACKGROUND', (0, 0), (-1, 0), TEAL),
        ('ALIGN', (0, 0), (-1, 0), 'LEFT'),
        ('BOTTOMPADDING', (0, 0), (-1, 0), 8),
        ('TOPPADDING', (0, 0), (-1, 0), 8),
        ('LEFTPADDING', (0, 0), (-1, -1), 8),
        ('RIGHTPADDING', (0, 0), (-1, -1), 8),

        # Body rows - alternating colors
        ('ROWBACKGROUNDS', (0, 1), (-1, -1), [HexColor("#ffffff"), HexColor("#f9fafb")]),
        ('VALIGN', (0, 0), (-1, -1), 'TOP'),
        ('BOTTOMPADDING', (0, 1), (-1, -1), 6),
        ('TOPPADDING', (0, 1), (-1, -1), 6),

        # Grid
        ('GRID', (0, 0), (-1, -1), 0.5, HexColor("#e5e7eb")),
    ])

    table.setStyle(style)
    return table


def is_markdown_preamble(line_index, line, lines):
    """Check if this line is part of the title/subtitle/date preamble we already handle on the cover page"""
    # Skip: # AMOS: The Operating System...
    if line.startswith("# AMOS:"):
        return True
    # Skip: ## A Strategic Thesis... (the subtitle)
    if line.startswith("## A Strategic Thesis"):
        return True
    # Skip: **April 2026 | AMOS Labs**
    if line.strip().startswith("**April 2026"):
        return True
    # Skip --- separators that appear in first 10 lines
    if line_index < 10 and line.strip() == "---":
        return True
    return False


def process_markdown_content(lines):
    """Process markdown lines into story elements and TOC entries"""
    story = []
    toc_entries = []
    i = 0

    # Define reusable styles
    part_style = ParagraphStyle(
        'PartHeading',
        fontName='Helvetica-Bold',
        fontSize=20,
        textColor=NAVY,
        spaceAfter=15,
        spaceBefore=20,
    )

    section_style = ParagraphStyle(
        'SectionHeading',
        fontName='Helvetica-Bold',
        fontSize=15,
        textColor=NAVY,
        spaceAfter=12,
        spaceBefore=15,
    )

    subsection_style = ParagraphStyle(
        'SubsectionHeading',
        fontName='Helvetica-Bold',
        fontSize=12,
        textColor=DARK_GRAY,
        spaceAfter=10,
        spaceBefore=10,
    )

    h4_style = ParagraphStyle(
        'H4Heading',
        fontName='Helvetica-Bold',
        fontSize=11,
        textColor=DARK_GRAY,
        spaceAfter=8,
        spaceBefore=8,
    )

    body_style = ParagraphStyle(
        'BodyText',
        fontName='Helvetica',
        fontSize=10.5,
        textColor=DARK_GRAY,
        spaceAfter=8,
        alignment=4,  # justify
        leading=14,
    )

    bullet_style = ParagraphStyle(
        'BulletItem',
        fontName='Helvetica',
        fontSize=10.5,
        textColor=DARK_GRAY,
        spaceAfter=5,
        leftIndent=25,
        bulletIndent=12,
        leading=14,
    )

    numbered_style = ParagraphStyle(
        'NumberedItem',
        fontName='Helvetica',
        fontSize=10.5,
        textColor=DARK_GRAY,
        spaceAfter=5,
        leftIndent=25,
        bulletIndent=12,
        leading=14,
    )

    code_style = ParagraphStyle(
        'Code',
        fontName='Courier',
        fontSize=8,
        textColor=DARK_GRAY,
        backColor=LIGHT_GRAY,
        spaceAfter=8,
        leftIndent=10,
        rightIndent=10,
        borderPadding=6,
    )

    quote_style = ParagraphStyle(
        'Blockquote',
        fontName='Helvetica-Oblique',
        fontSize=10.5,
        textColor=GRAY,
        spaceAfter=10,
        leftIndent=20,
        borderColor=TEAL,
        borderLeft=3,
        borderPadding=10,
        leading=14,
    )

    callout_style = ParagraphStyle(
        'Callout',
        fontName='Helvetica',
        fontSize=10.5,
        textColor=DARK_GRAY,
        spaceAfter=10,
        leftIndent=15,
        borderColor=TEAL,
        borderLeft=4,
        borderPadding=10,
        backColor=HexColor("#f0fdf9"),
        leading=14,
    )

    while i < len(lines):
        line = lines[i]

        # Skip preamble lines (already on cover page)
        if is_markdown_preamble(i, line, lines):
            i += 1
            continue

        # Part headings (## Part X) — major sections, add page break before
        if line.startswith("## Part"):
            heading_text = line[3:]
            story.append(PageBreak())
            story.append(Spacer(1, 0.3 * inch))
            story.append(Paragraph(heading_text, part_style))
            toc_entries.append((1, heading_text))
            i += 1
            continue

        # Other ## headings — section level
        if line.startswith("## "):
            heading_text = line[3:]
            story.append(Spacer(1, 0.15 * inch))
            story.append(Paragraph(heading_text, section_style))
            toc_entries.append((1, heading_text))
            i += 1
            continue

        # ### headings
        if line.startswith("### "):
            heading_text = line[4:]
            story.append(Spacer(1, 0.1 * inch))
            story.append(Paragraph(heading_text, subsection_style))
            toc_entries.append((2, heading_text))
            i += 1
            continue

        # #### headings
        if line.startswith("#### "):
            heading_text = line[5:]
            story.append(Paragraph(heading_text, h4_style))
            i += 1
            continue

        # Horizontal rules (not in preamble)
        if line.strip() == "---":
            story.append(Spacer(1, 0.1 * inch))
            i += 1
            continue

        # Code blocks
        if line.strip().startswith("```"):
            code_lines = []
            i += 1
            while i < len(lines) and not lines[i].strip().startswith("```"):
                code_lines.append(lines[i])
                i += 1
            i += 1  # skip closing ```

            code_text = '\n'.join(code_lines).strip()
            if code_text:
                # Escape for Preformatted
                code_text = code_text.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')
                story.append(Spacer(1, 0.05 * inch))
                story.append(Preformatted(code_text, code_style))
                story.append(Spacer(1, 0.05 * inch))
            continue

        # Tables
        if line.strip().startswith("|"):
            table_lines = []
            while i < len(lines) and lines[i].strip().startswith("|"):
                table_lines.append(lines[i])
                i += 1

            table_data = parse_table(table_lines)
            if table_data and len(table_data) >= 2:
                table = create_styled_table(table_data)
                if table:
                    story.append(Spacer(1, 0.1 * inch))
                    story.append(table)
                    story.append(Spacer(1, 0.1 * inch))
            continue

        # Blockquotes
        if line.startswith(">"):
            quote_lines = []
            while i < len(lines) and lines[i].startswith(">"):
                quote_text = lines[i][1:].strip()
                quote_lines.append(quote_text)
                i += 1

            combined = " ".join(quote_lines)
            combined = format_inline_text(combined)
            story.append(Paragraph(combined, quote_style))
            continue

        # Numbered lists (1. 2. 3. etc)
        if re.match(r'^\d+\.\s', line.strip()):
            while i < len(lines) and re.match(r'^\d+\.\s', lines[i].strip()):
                item_match = re.match(r'^(\d+)\.\s+(.*)', lines[i].strip())
                if item_match:
                    num = item_match.group(1)
                    item_text = format_inline_text(item_match.group(2))
                    story.append(Paragraph(f"{num}. {item_text}", numbered_style))
                i += 1
            story.append(Spacer(1, 0.03 * inch))
            continue

        # Bullet lists
        if line.strip().startswith("- "):
            while i < len(lines) and lines[i].strip().startswith("- "):
                item_text = lines[i].strip()[2:].strip()
                item_text = format_inline_text(item_text)
                story.append(Paragraph(f"• {item_text}", bullet_style))
                i += 1
            story.append(Spacer(1, 0.03 * inch))
            continue

        # Regular paragraphs
        if line.strip():
            # Collect multi-line paragraphs
            para_lines = []
            while i < len(lines) and lines[i].strip() and not lines[i].startswith("#") and not lines[i].startswith("|") and not lines[i].startswith(">") and not lines[i].startswith("```") and not lines[i].strip() == "---" and not lines[i].strip().startswith("- ") and not re.match(r'^\d+\.\s', lines[i].strip()):
                para_lines.append(lines[i].strip())
                i += 1

            full_text = " ".join(para_lines)
            formatted_text = format_inline_text(full_text)

            # Special callout for key AMOS statement
            if "AMOS" in formatted_text and "deliberate intervention" in formatted_text:
                story.append(Paragraph(formatted_text, callout_style))
            else:
                story.append(Paragraph(formatted_text, body_style))
            continue

        # Empty lines
        story.append(Spacer(1, 0.03 * inch))
        i += 1

    return story, toc_entries


def generate_pdf(input_file, output_file):
    """Main PDF generation function"""
    print(f"Reading markdown from: {input_file}")

    # Parse markdown
    with open(input_file, 'r', encoding='utf-8') as f:
        content = f.read()
    lines = content.split('\n')

    # Process content to get story elements and TOC
    content_story, toc_entries = process_markdown_content(lines)

    # Build final story: cover -> TOC -> content
    final_story = []
    create_title_page(final_story)
    create_toc(final_story, toc_entries)
    final_story.extend(content_story)

    # Create PDF document
    doc = SimpleDocTemplate(
        output_file,
        pagesize=letter,
        rightMargin=0.75 * inch,
        leftMargin=0.75 * inch,
        topMargin=0.85 * inch,
        bottomMargin=0.85 * inch,
        title="AMOS Strategy Document",
        author="AMOS Labs",
        subject="Strategic Thesis and Overview",
        creator="AMOS Strategy PDF Generator",
    )

    # Build with header/footer
    header_footer = PDFHeaderFooter()
    doc.build(final_story, onFirstPage=header_footer.on_page, onLaterPages=header_footer.on_page)
    print(f"PDF generated successfully: {output_file}")
    return True


def main():
    """Main entry point"""
    script_dir = Path(__file__).parent
    repo_root = script_dir.parent

    input_file = repo_root / "docs" / "AMOS_THESIS_AND_STRATEGY.md"
    output_file_1 = repo_root / "docs" / "AMOS_Strategy_Document.pdf"
    output_file_2 = repo_root / "AMOS_Strategy_Document.pdf"

    if not input_file.exists():
        print(f"Error: Input file not found: {input_file}")
        sys.exit(1)

    try:
        generate_pdf(str(input_file), str(output_file_1))
        print(f"Created: {output_file_1}")
    except Exception as e:
        print(f"Error generating PDF: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)

    # Copy to second location
    try:
        import shutil
        shutil.copy2(output_file_1, output_file_2)
        print(f"Copied to: {output_file_2}")
    except Exception as e:
        print(f"Warning: Could not copy to second location: {e}")

    if output_file_1.exists():
        size = output_file_1.stat().st_size
        print(f"\nPDF successfully generated ({size:,} bytes)")
        return 0
    else:
        print("Error: PDF file was not created")
        return 1


if __name__ == "__main__":
    sys.exit(main())
