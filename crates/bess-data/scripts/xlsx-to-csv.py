#!/usr/bin/env python3
"""Turns regelleistung.net's result files (xlsx) into plain CSV, one per
file, beside them, so `compile-balancing` can read them without a
spreadsheet library.

    crates/bess-data/scripts/xlsx-to-csv.py <dir>

Standard library only (zipfile and the XML inside). Every cell is written
as the sheet carries it; shared strings and inline strings are resolved.
"""
import glob, html, os, re, sys, zipfile


def rows(path):
    z = zipfile.ZipFile(path)
    names = z.namelist()
    shared = []
    if "xl/sharedStrings.xml" in names:
        xml = z.read("xl/sharedStrings.xml").decode()
        shared = [html.unescape(re.sub("<[^>]+>", "", m)) for m in re.findall(r"<si>(.*?)</si>", xml, re.S)]
    sheet = z.read("xl/worksheets/sheet1.xml").decode()
    out = []
    for row in re.findall(r"<row[^>]*>(.*?)</row>", sheet, re.S):
        cells = {}
        for attrs, body in re.findall(r"<c([^>]*)>(.*?)</c>", row, re.S):
            col = re.search(r'r="([A-Z]+)\d+"', attrs).group(1)
            value = re.search(r"<v>(.*?)</v>", body)
            value = value.group(1) if value else ""
            if 't="s"' in attrs:
                value = shared[int(value)]
            elif 't="inlineStr"' in attrs:
                t = re.search(r"<t[^>]*>(.*?)</t>", body)
                value = html.unescape(t.group(1)) if t else ""
            cells[col] = value
        out.append(cells)
    return out


def column_index(name):
    n = 0
    for ch in name:
        n = n * 26 + ord(ch) - 64
    return n


def main(directory):
    count = 0
    for path in sorted(glob.glob(os.path.join(directory, "*.xlsx"))):
        table = rows(path)
        if not table:
            continue
        columns = sorted({c for r in table for c in r}, key=column_index)
        with open(path[:-5] + ".csv", "w", encoding="utf-8") as f:
            for r in table:
                f.write(";".join(r.get(c, "").replace(";", ",") for c in columns) + "\n")
        count += 1
    print(f"{count} files converted under {directory}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "regelleistung-raw")
