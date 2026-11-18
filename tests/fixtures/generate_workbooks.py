#!/usr/bin/env python3
"""Reproducible generator for binary real-world workbook fixtures under tests/fixtures/workbooks/."""

from __future__ import annotations

import io
import struct
import zipfile
from pathlib import Path

import openpyxl
from openpyxl.comments import Comment
from openpyxl.workbook.defined_name import DefinedName


OUT_DIR = Path(__file__).resolve().parent / "workbooks"


def ensure_out_dir() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)


def build_messy_enterprise_xlsx() -> Path:
    """Builds a complex real .xlsx with visible/hidden/veryHidden sheets, multi-table, merges, formulas & comments."""
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = "Sales & KPIs"
    ws.freeze_panes = "A4"

    # Title banner + blank row + merged super-header + detailed column header
    ws["A1"] = "FY2026 Regional Sales Report"
    ws.merge_cells("A1:E1")
    ws["A3"] = "Account Info"
    ws.merge_cells("A3:B3")
    ws["C3"] = "Financials"
    ws.merge_cells("C3:E3")

    headers = ["Account ID", "Region", "Revenue", "Units", "Audit Formula"]
    for idx, name in enumerate(headers, start=1):
        ws.cell(row=4, column=idx, value=name)

    rows = [
        ("00105", "القاهرة", 12500, 25, "=SUM(C5:C5)"),
        ("00106", "الإسكندرية", 8400, 14, "=AVERAGE(C5:C6)"),
        ("00107", "الجيزة", 6100, 11, '=IF(D7>10,"HIGH","LOW")'),
        ("00108", "الدلتا", 4200, 8, "=TODAY()"),
        ("00109", "أسوان", 1900, 4, "=[ExternalBudget.xlsx]Budget!B2"),
    ]
    for r_off, (acct, region, rev, units, formula) in enumerate(rows, start=5):
        c_acct = ws.cell(row=r_off, column=1, value=acct)
        c_acct.number_format = "@"
        ws.cell(row=r_off, column=2, value=region)
        ws.cell(row=r_off, column=3, value=rev).number_format = "#,##0.00"
        ws.cell(row=r_off, column=4, value=units)
        ws.cell(row=r_off, column=5, value=formula)

    ws["B5"].comment = Comment("Verified regional HQ", "SafiAudit")
    ws["A5"].hyperlink = "https://safidata.local/accounts/00105"

    # Subtotal & grand total rows that must be separated from analytical rows
    ws["A10"] = "Subtotal"
    ws["C10"] = "=SUM(C5:C9)"
    ws["A11"] = "Grand Total"
    ws["C11"] = 33100
    ws["A12"] = "Source: Audited ERP Ledger FY2026"

    # Second table on the same worksheet after blank rows
    ws["A15"] = "Target Name"
    ws["B15"] = "Quota"
    ws["C15"] = "Error Check"
    ws["A16"] = "North"
    ws["B16"] = 20000
    ws["C16"] = "=100/0"
    ws["A17"] = "South"
    ws["B17"] = 15000
    ws["C17"] = "=B16+B17"

    ws.auto_filter.ref = "A4:E9"

    # Hidden sheet
    ws_hidden = wb.create_sheet("Lookup_Rates")
    ws_hidden.sheet_state = "hidden"
    ws_hidden.append(["Code", "Multiplier"])
    ws_hidden.append(["01", 1.15])
    ws_hidden.append(["02", 1.25])

    # VeryHidden sheet
    ws_very = wb.create_sheet("Secret_Params")
    ws_very.sheet_state = "veryHidden"
    ws_very.append(["Param", "Value"])
    ws_very.append(["TaxRate", 0.14])

    wb.defined_names.add(DefinedName("TaxRate", attr_text="Secret_Params!$B$2"))
    wb.defined_names.add(DefinedName("_xlnm.Print_Area", attr_text="'Sales & KPIs'!$A$4:$E$11"))

    target = OUT_DIR / "messy_enterprise.xlsx"
    wb.save(target)
    return target


def build_macro_1904_xlsm() -> Path:
    """Builds a real OOXML .xlsm containing vbaProject.bin and Macintosh 1904 date system."""
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = "Treasury"
    ws.append(["BatchID", "Amount", "Tax"])
    ws.append(["0091", 1000, "=B2*0.14"])
    ws.append(["0092", 2000, "=B3*0.14"])
    ws.append(["Total", "=SUM(B2:B3)", "=SUM(C2:C3)"])

    buf = io.BytesIO()
    wb.save(buf)
    buf.seek(0)

    out_path = OUT_DIR / "macro_mac1904.xlsm"
    with zipfile.ZipFile(buf, "r") as zin, zipfile.ZipFile(out_path, "w", zipfile.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == "xl/workbook.xml":
                text = data.decode("utf-8")
                if 'date1904=' not in text:
                    if "<workbookPr" in text:
                        text = text.replace("<workbookPr", '<workbookPr date1904="1"')
                    else:
                        text = text.replace("<sheets>", '<workbookPr date1904="1"/><sheets>')
                data = text.encode("utf-8")
            zout.writestr(item, data)
        # Inject dummy VBA macro binary so SafiData neutralizes & audits it
        zout.writestr("xl/vbaProject.bin", b"DUMMY_VBA_MACRO_STREAM_NOT_EXECUTED")
    return out_path


def build_ods_fixture() -> Path:
    """Builds a standards-compliant OpenDocument Spreadsheet (.ods) ZIP container."""
    out_path = OUT_DIR / "bilingual_arabic.ods"
    mimetype = "application/vnd.oasis.opendocument.spreadsheet"
    manifest = """<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2">
  <manifest:file-entry manifest:full-path="/" manifest:version="1.2" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/>
  <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
  <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"""
    styles = """<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
  xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0">
  <office:styles/>
  <office:automatic-styles>
    <style:style style:name="HiddenSheet" style:family="table">
      <style:table-properties table:display="false"/>
    </style:style>
  </office:automatic-styles>
</office:document-styles>"""
    content = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
        'xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" '
        'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
        'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
        'office:version="1.2">'
        '<office:automatic-styles>'
        '<style:style style:name="HiddenSheet" style:family="table">'
        '<style:table-properties table:display="false"/>'
        '</style:style>'
        '</office:automatic-styles>'
        '<office:body><office:spreadsheet>'
        '<table:table table:name="الإيرادات">'
        '<table:table-row>'
        '<table:table-cell office:value-type="string"><text:p>الكود</text:p></table:table-cell>'
        '<table:table-cell office:value-type="string"><text:p>المحافظة</text:p></table:table-cell>'
        '<table:table-cell office:value-type="string"><text:p>المبلغ</text:p></table:table-cell>'
        '</table:table-row>'
        '<table:table-row>'
        '<table:table-cell office:value-type="string"><text:p>00712</text:p></table:table-cell>'
        '<table:table-cell office:value-type="string"><text:p>القاهرة</text:p></table:table-cell>'
        '<table:table-cell office:value-type="float" office:value="4500"><text:p>4500</text:p></table:table-cell>'
        '</table:table-row>'
        '<table:table-row>'
        '<table:table-cell office:value-type="string"><text:p>00713</text:p></table:table-cell>'
        '<table:table-cell office:value-type="string"><text:p>الجيزة</text:p></table:table-cell>'
        '<table:table-cell office:value-type="float" office:value="3200"><text:p>3200</text:p></table:table-cell>'
        '</table:table-row>'
        '</table:table>'
        '<table:table table:name="ODS_Hidden" table:style-name="HiddenSheet">'
        '<table:table-row>'
        '<table:table-cell office:value-type="string"><text:p>Key</text:p></table:table-cell>'
        '<table:table-cell office:value-type="string"><text:p>Val</text:p></table:table-cell>'
        '</table:table-row>'
        '<table:table-row>'
        '<table:table-cell office:value-type="string"><text:p>rate</text:p></table:table-cell>'
        '<table:table-cell office:value-type="float" office:value="12"><text:p>12</text:p></table:table-cell>'
        '</table:table-row>'
        '</table:table>'
        '</office:spreadsheet></office:body>'
        '</office:document-content>'
    )
    with zipfile.ZipFile(out_path, "w") as zout:
        zinfo = zipfile.ZipInfo("mimetype")
        zinfo.compress_type = zipfile.ZIP_STORED
        zout.writestr(zinfo, mimetype.encode("utf-8"))
        zout.writestr("META-INF/manifest.xml", manifest.encode("utf-8"), zipfile.ZIP_DEFLATED)
        zout.writestr("styles.xml", styles.encode("utf-8"), zipfile.ZIP_DEFLATED)
        zout.writestr("content.xml", content.encode("utf-8"), zipfile.ZIP_DEFLATED)
    return out_path


def _biff_rec(rec_id: int, payload: bytes) -> bytes:
    return struct.pack("<HH", rec_id, len(payload)) + payload


def _biff_unicode_short(text: str) -> bytes:
    # 1-byte len + flags=1 (UTF-16LE) + UTF-16LE bytes
    raw = text.encode("utf-16-le")
    return struct.pack("<BB", len(text), 1) + raw


def _biff_unicode_long(text: str) -> bytes:
    # 2-byte len + flags=1 (UTF-16LE) + UTF-16LE bytes
    raw = text.encode("utf-16-le")
    return struct.pack("<HB", len(text), 1) + raw


def build_biff8_xls() -> Path:
    """Builds a minimal standards-compliant BIFF8 (.xls) OLE2 Compound Binary file with visible + hidden sheets."""
    strings = ["Region", "Amount", "Cairo", "Giza", "Code", "00501"]

    # Globals substream
    rec_bof_globals = _biff_rec(0x0809, struct.pack("<HHHHII", 0x0600, 0x0005, 0x0DBB, 0x07CC, 0, 0))
    rec_codepage = _biff_rec(0x0042, struct.pack("<H", 1200))
    # SST record: total_strings(u32), unique_strings(u32), encoded strings
    sst_payload = bytearray(struct.pack("<II", len(strings), len(strings)))
    for s in strings:
        sst_payload.extend(_biff_unicode_long(s))
    rec_sst = _biff_rec(0x00FC, bytes(sst_payload))
    rec_eof = _biff_rec(0x000A, b"")

    # Sheet 1 substream ("MainSheet")
    def make_sheet(rows_spec: list[tuple[int, int, str, int | float]]) -> bytes:
        s_bof = _biff_rec(0x0809, struct.pack("<HHHHII", 0x0600, 0x0010, 0x0DBB, 0x07CC, 0, 0))
        dims = _biff_rec(0x0200, struct.pack("<IIHHH", 0, 3, 0, 2, 0))
        cells = bytearray()
        for r, c, kind, val in rows_spec:
            if kind == "sst":
                cells.extend(_biff_rec(0x00FD, struct.pack("<HHHI", r, c, 0, int(val))))
            else:
                cells.extend(_biff_rec(0x0203, struct.pack("<HHHd", r, c, 0, float(val))))
        return s_bof + dims + bytes(cells) + rec_eof

    sheet1 = make_sheet([
        (0, 0, "sst", 0),  # Region
        (0, 1, "sst", 1),  # Amount
        (1, 0, "sst", 2),  # Cairo
        (1, 1, "num", 1200.0),
        (2, 0, "sst", 3),  # Giza
        (2, 1, "num", 800.0),
    ])
    sheet2 = make_sheet([
        (0, 0, "sst", 4),  # Code
        (0, 1, "sst", 1),  # Amount
        (1, 0, "sst", 5),  # 00501
        (1, 1, "num", 50.0),
    ])

    bs1_name = _biff_unicode_short("MainSheet")
    bs2_name = _biff_unicode_short("HiddenBiff")
    # BOUNDSHEET fixed prefix = 6 bytes before pos update, name follows
    # Calculate exact offset of sheet1 & sheet2 inside Workbook stream
    placeholder_bs1 = _biff_rec(0x0085, struct.pack("<IBB", 0, 0, 0) + bs1_name)
    placeholder_bs2 = _biff_rec(0x0085, struct.pack("<IBB", 0, 1, 0) + bs2_name)
    globals_len = len(rec_bof_globals + rec_codepage + placeholder_bs1 + placeholder_bs2 + rec_sst + rec_eof)
    offset_s1 = globals_len
    offset_s2 = globals_len + len(sheet1)

    rec_bs1 = _biff_rec(0x0085, struct.pack("<IBB", offset_s1, 0x00, 0x00) + bs1_name)
    rec_bs2 = _biff_rec(0x0085, struct.pack("<IBB", offset_s2, 0x01, 0x00) + bs2_name)  # visible=1 (Hidden)
    workbook_stream = rec_bof_globals + rec_codepage + rec_bs1 + rec_bs2 + rec_sst + rec_eof + sheet1 + sheet2

    # Wrap workbook_stream in a valid 512-byte sector OLE2 Compound Binary Container
    sector_size = 512
    # Pad workbook stream to sector boundary (>= 4096 bytes so it stays regular-sector stream, avoiding MiniStream)
    if len(workbook_stream) < 4096:
        workbook_stream += b"\x00" * (4096 - len(workbook_stream))
    wb_sectors = (len(workbook_stream) + sector_size - 1) // sector_size
    wb_padded = workbook_stream + b"\x00" * (wb_sectors * sector_size - len(workbook_stream))

    # Layout:
    # Sector 0: Directory sector
    # Sector 1..wb_sectors: Workbook stream
    # Sector 1+wb_sectors: FAT sector (1 sector covers up to 128 sectors = 64 KiB)
    dir_sector_idx = 0
    wb_start_sec = 1
    fat_sector_idx = 1 + wb_sectors
    total_sectors = fat_sector_idx + 1
    assert total_sectors <= 128

    fat = [0xFFFFFFFF] * 128  # FREESECT
    fat[dir_sector_idx] = 0xFFFFFFFE  # ENDOFCHAIN for directory
    for i in range(wb_sectors):
        sec = wb_start_sec + i
        fat[sec] = (sec + 1) if i + 1 < wb_sectors else 0xFFFFFFFE
    fat[fat_sector_idx] = 0xFFFFFFFD  # FATSECT
    fat_bytes = struct.pack("<128I", *fat)

    # Directory entry (128 bytes): 64s + H + B + B + I + I + I + 16s + I + Q + Q + I + Q = 128 bytes
    def pack_dir_entry(name: str, obj_type: int, child_id: int, start_sec: int, size: int) -> bytes:
        utf16 = name.encode("utf-16-le") + b"\x00\x00" if name else b""
        name_buf = (utf16 + b"\x00" * 64)[:64]
        return struct.pack(
            "<64sHBBIII16sIQQIQ",
            name_buf,
            len(utf16),
            obj_type,
            1,
            0xFFFFFFFF,
            0xFFFFFFFF,
            child_id,
            b"\x00" * 16,
            0,
            0,
            0,
            start_sec,
            size,
        )

    root_entry = pack_dir_entry("Root Entry", 5, 1, 0xFFFFFFFE, 0)
    wb_entry = pack_dir_entry("Workbook", 2, 0xFFFFFFFF, wb_start_sec, len(workbook_stream))
    empty_entry = pack_dir_entry("", 0, 0xFFFFFFFF, 0, 0)
    dir_bytes = root_entry + wb_entry + empty_entry + empty_entry
    assert len(dir_bytes) == 512

    # CFBF 512-byte header (76-byte prefix + 436-byte DIFAT[109])
    difat = [fat_sector_idx] + [0xFFFFFFFF] * 108
    header = struct.pack(
        "<8s16sHHHHH6sIIIIIIIII",
        b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1",
        b"\x00" * 16,
        0x003E,      # minor ver
        0x0003,      # major ver (v3, 512B sectors)
        0xFFFE,      # little endian
        9,           # 2^9 = 512B
        6,           # 2^6 = 64B mini sector
        b"\x00" * 6, # reserved
        0,           # total dir sectors (0 for v3)
        1,           # num FAT sectors
        dir_sector_idx,
        0,           # transaction signature
        4096,        # mini stream cutoff
        0xFFFFFFFE,  # no mini FAT
        0,
        0xFFFFFFFE,  # no DIFAT chain
        0,
    ) + struct.pack("<109I", *difat)
    assert len(header) == 512

    out_path = OUT_DIR / "legacy_biff8.xls"
    out_path.write_bytes(header + dir_bytes + wb_padded + fat_bytes)
    return out_path


def build_csv_and_adversarial_fixtures() -> None:
    """Writes Windows-1256 CSV, TSV, wide streaming XLSX, corrupt XLSX, and ZIP bomb XLSX."""
    csv_text = "رقم_الحساب,المدينة,القيمة\n00123,القاهرة,1500.5\n00124,الإسكندرية,980.25\n"
    (OUT_DIR / "arabic_cp1256.csv").write_bytes(csv_text.encode("cp1256"))

    tsv_text = "code\tregion\tamount\n0081\tCairo\t550\n0082\tGiza\t450\n"
    (OUT_DIR / "tab_delimited.tsv").write_bytes(tsv_text.encode("utf-8-sig"))

    # Large 500-row XLSX for cancellation & streaming test
    wb = openpyxl.Workbook(write_only=True)
    ws = wb.create_sheet("Transactions")
    ws.append(["TxID", "Category", "Amount"])
    for i in range(1, 501):
        ws.append([f"{i:05d}", f"Cat_{i % 7}", (i * 17) % 1000 + 100])
    wb.save(OUT_DIR / "wide_500_rows.xlsx")

    # Corrupt / truncated zip container
    (OUT_DIR / "corrupt_truncated.xlsx").write_bytes(b"PK\x03\x04CORRUPTED_BYTES_ONLY")

    # Inflated ZIP bomb: minimal deflated 60 MiB zeros inside small .xlsx payload (> MAX_UNCOMPRESSED_BYTES or extreme ratio)
    bomb_path = OUT_DIR / "zip_bomb_oversized.xlsx"
    payload = b"A" * (12 * 1024 * 1024)
    with zipfile.ZipFile(bomb_path, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as zout:
        zout.writestr("[Content_Types].xml", b"<Types/>")
        zout.writestr("xl/worksheets/sheet1.xml", payload)


def main() -> None:
    ensure_out_dir()
    created = [
        build_messy_enterprise_xlsx(),
        build_macro_1904_xlsm(),
        build_ods_fixture(),
        build_biff8_xls(),
    ]
    build_csv_and_adversarial_fixtures()
    for p in sorted(OUT_DIR.iterdir()):
        print(f"Generated fixture: {p.name} ({p.stat().st_size} bytes)")
    assert created


if __name__ == "__main__":
    main()
