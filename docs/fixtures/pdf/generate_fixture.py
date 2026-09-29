"""Generate the self-authored PDF used by the Phase 1 feasibility spike."""

from io import BytesIO

from fontTools.ttLib import TTFont
from pypdf import PdfWriter
from pypdf.generic import (
    ArrayObject,
    DictionaryObject,
    NameObject,
    NumberObject,
    StreamObject,
    TextStringObject,
)


OUTPUT = "technical-fixture.pdf"
TRANSLATED_OUTPUT = "translated-fixture.pdf"
FONT_PATH = "C:/Windows/Fonts/arial.ttf"


class EmbeddedFont:
    def __init__(self, path):
        self.font = TTFont(path)
        self.cmap = self.font.getBestCmap()
        self.glyph_ids = {
            name: index for index, name in enumerate(self.font.getGlyphOrder())
        }
        self.used = {}
        self.font_ref = None

    def encode(self, value):
        encoded = bytearray()
        for character in value:
            glyph_name = self.cmap.get(ord(character), ".notdef")
            glyph_id = self.glyph_ids[glyph_name]
            self.used[glyph_id] = character
            encoded.extend(glyph_id.to_bytes(2, "big"))
        return encoded.hex().upper()

    def add_objects(self, writer):
        font_bytes = BytesIO()
        self.font.save(font_bytes)
        font_file = StreamObject()
        font_file._data = font_bytes.getvalue()
        font_file[NameObject("/Length1")] = NumberObject(len(font_file._data))
        font_file_ref = writer._add_object(font_file)

        descriptor = DictionaryObject(
            {
                NameObject("/Type"): NameObject("/FontDescriptor"),
                NameObject("/FontName"): NameObject("/Arial"),
                NameObject("/Flags"): NumberObject(32),
                NameObject("/FontBBox"): ArrayObject(
                    [NumberObject(value) for value in (-665, -325, 2000, 2069)]
                ),
                NameObject("/ItalicAngle"): NumberObject(0),
                NameObject("/Ascent"): NumberObject(1854),
                NameObject("/Descent"): NumberObject(-434),
                NameObject("/CapHeight"): NumberObject(1462),
                NameObject("/StemV"): NumberObject(80),
                NameObject("/FontFile2"): font_file_ref,
            }
        )
        descriptor_ref = writer._add_object(descriptor)

        cid_font = DictionaryObject(
            {
                NameObject("/Type"): NameObject("/Font"),
                NameObject("/Subtype"): NameObject("/CIDFontType2"),
                NameObject("/BaseFont"): NameObject("/Arial"),
                NameObject("/CIDSystemInfo"): DictionaryObject(
                    {
                        NameObject("/Registry"): TextStringObject("Adobe"),
                        NameObject("/Ordering"): TextStringObject("Identity"),
                        NameObject("/Supplement"): NumberObject(0),
                    }
                ),
                NameObject("/FontDescriptor"): descriptor_ref,
                NameObject("/CIDToGIDMap"): NameObject("/Identity"),
                NameObject("/DW"): NumberObject(1000),
            }
        )
        cid_font_ref = writer._add_object(cid_font)

        cmap_lines = [
            "/CIDInit /ProcSet findresource begin",
            "12 dict begin",
            "begincmap",
            "/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def",
            "/CMapName /ArialUnicode def",
            "/CMapType 2 def",
            "1 begincodespacerange",
            "<0000> <FFFF>",
            "endcodespacerange",
        ]
        entries = []
        for glyph_id, character in sorted(self.used.items()):
            unicode_hex = character.encode("utf-16-be").hex().upper()
            entries.append(f"<{glyph_id:04X}> <{unicode_hex}>")
        for start in range(0, len(entries), 100):
            batch = entries[start : start + 100]
            cmap_lines.append(f"{len(batch)} beginbfchar")
            cmap_lines.extend(batch)
            cmap_lines.append("endbfchar")
        cmap_lines.extend(
            [
                "endcmap",
                "CMapName currentdict /CMap defineresource pop",
                "end",
                "end",
            ]
        )
        to_unicode = StreamObject()
        to_unicode._data = "\n".join(cmap_lines).encode("ascii")
        to_unicode_ref = writer._add_object(to_unicode)

        self.font_ref = writer._add_object(
            DictionaryObject(
                {
                    NameObject("/Type"): NameObject("/Font"),
                    NameObject("/Subtype"): NameObject("/Type0"),
                    NameObject("/BaseFont"): NameObject("/Arial"),
                    NameObject("/Encoding"): NameObject("/Identity-H"),
                    NameObject("/DescendantFonts"): ArrayObject([cid_font_ref]),
                    NameObject("/ToUnicode"): to_unicode_ref,
                }
            )
        )


def text(font, x, y, value, size=11):
    return f"BT /F1 {size} Tf {x} {y} Td <{font.encode(value)}> Tj ET\n"


def page_content(font, page_number, translated=False):
    left_lines = (
        ("Приёмник преобразует входной сигнал перед вторым каскадом.", ""),
        ("Сохраняйте частоту дискретизации 48 кГц и", ""),
    ) if translated else (
        ("The receiver converts the input signal", "before the second stage."),
        ("", ""),
    )
    right_lines = (
        ("Выходной сигнал измеряется с частотой 48 кГц.", ""),
        ("Порядок блоков должен быть сохранён.", ""),
    ) if translated else (
        ("The output is sampled at 48 kHz.", ""),
        ("Keep the order of the blocks.", ""),
    )
    parts = [
        text(font, 42, 750, "Signal Path Feasibility Fixture", 16),
        text(font, 42, 728, f"Technical sample page {page_number}  |  identifier ADC-02", 10),
        "0.8 w 42 712 m 550 712 l S\n",
        text(font, 42, 680, "LEFT COLUMN", 11),
        text(font, 42, 660, left_lines[0][0], 10),
        text(font, 42, 644, left_lines[0][1], 10),
        text(font, 42, 610, "RIGHT COLUMN", 11),
        text(font, 310, 660, right_lines[0][0], 10),
        text(font, 310, 644, right_lines[0][1], 10),
        "0.2 0.4 0.7 RG 2 w 42 565 m 170 565 l 210 525 l 340 525 l S\n",
        text(font, 42, 500, "Рисунок 1. Векторный путь сигнала" if translated else "Figure 1. Vector signal path", 9),
        "0.95 0.95 0.95 rg 42 430 280 45 re f\n",
        "0 g 1 w 42 430 280 45 re S\n",
        text(font, 52, 447, "Вход     x[n] = sin(2*pi*f*t)     Выход" if translated else "Input     x[n] = sin(2*pi*f*t)     Output", 10),
        text(font, 42, 390, "Таблица 1. Параметры" if translated else "Table 1. Parameters", 10),
        "42 330 280 42 re S 182 330 m 182 372 l S\n",
        text(font, 52, 350, "частота дискретизации" if translated else "sample rate", 9),
        text(font, 192, 350, "48 кГц" if translated else "48 kHz", 9),
        text(font, 42, 300, "https://example.invalid/signal-path", 9),
        text(font, 42, 265, "Повторяющийся нижний колонтитул: self-authored fixture" if translated else "Repeated footer: self-authored fixture", 8),
        text(font, 42, 220, "Русский текст проверяет embedded Cyrillic font.", 10),
    ]
    return "".join(parts).encode("ascii")


def add_page(writer, font, content):
    page = writer.add_blank_page(width=595, height=842)
    resources = DictionaryObject({NameObject("/Font"): DictionaryObject({NameObject("/F1"): font.font_ref})})
    page[NameObject("/Resources")] = resources
    stream = StreamObject()
    stream._data = content
    page[NameObject("/Contents")] = writer._add_object(stream)


def write_document(output_path, translated=False):
    writer = PdfWriter()
    embedded_font = EmbeddedFont(FONT_PATH)
    page_contents = [
        page_content(embedded_font, number, translated) for number in (1, 2)
    ]
    embedded_font.add_objects(writer)
    for content in page_contents:
        add_page(writer, embedded_font, content)
    with open(output_path, "wb") as output:
        writer.write(output)


write_document(OUTPUT)
write_document(TRANSLATED_OUTPUT, translated=True)
