
```bash
export CWD="$(pwd)"
export PARENT="$(dirname "$CWD")"
echo "cwd: $CWD"
echo "parent: $PARENT"
```

Google Bookmarks to CSV Gist
```bash
mkdir -p $PARENT/gists
curl -o $PARENT/gists/google_bookmarks_to_csv.py https://gist.githubusercontent.com/codingforentrepreneurs/916be33f515589df486c80ca9d07ca0d/raw/6e25751789a6b7f98a1b9b0a4195e1133f6e5c02/google_bookmarks_to_csv.py
```