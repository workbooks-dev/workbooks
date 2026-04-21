"""Convert a Chrome/Google bookmarks HTML export into a flat CSV.

Usage:
    uv run google_bookmarks_to_csv.py <input.html> <output.csv>
    python3 google_bookmarks_to_csv.py <input.html> <output.csv>

"""
import csv
import datetime as dt
import html
import pathlib
import sys
from html.parser import HTMLParser


def to_iso8601(value: str) -> str:
    if not value:
        return ""
    return dt.datetime.fromtimestamp(int(value), tz=dt.timezone.utc).isoformat()


class BookmarkParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.folder_stack: list[str] = []
        self.pending_folder: str | None = None
        self.current: tuple[str, dict[str, str], list[str]] | None = None
        self.rows: list[dict[str, str]] = []

    def handle_starttag(self, tag: str, attrs) -> None:
        attributes = dict(attrs)
        if tag == "h3":
            self.current = ("h3", attributes, [])
        elif tag == "a":
            self.current = ("a", attributes, [])

    def handle_data(self, data: str) -> None:
        if self.current is not None:
            self.current[2].append(data)

    def handle_endtag(self, tag: str) -> None:
        if self.current is None:
            if tag == "dl" and self.pending_folder is not None:
                self.folder_stack.append(self.pending_folder)
                self.pending_folder = None
            elif tag == "dl" and self.folder_stack:
                self.folder_stack.pop()
            return

        kind, attributes, parts = self.current
        text = html.unescape("".join(parts).strip())

        if tag == "h3" and kind == "h3":
            self.pending_folder = text
            self.current = None
            return

        if tag == "a" and kind == "a":
            self.rows.append(
                {
                    "folder_path": " / ".join(self.folder_stack),
                    "title": text,
                    "url": attributes.get("href", ""),
                    "add_date": to_iso8601(attributes.get("add_date", "")),
                }
            )
            self.current = None


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: uv run gists/google_bookmarks_to_csv.py <input.html> <output.csv>")
        return 1

    input_path = pathlib.Path(sys.argv[1])
    output_path = pathlib.Path(sys.argv[2])

    parser = BookmarkParser()
    parser.feed(input_path.read_text(encoding="utf-8"))

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", newline="", encoding="utf-8") as file:
        writer = csv.DictWriter(
            file,
            fieldnames=["folder_path", "title", "url", "add_date"],
        )
        writer.writeheader()
        writer.writerows(parser.rows)

    print(f"wrote {len(parser.rows)} rows to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
