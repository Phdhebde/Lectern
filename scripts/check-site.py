"""Static checks of the generated project site (SEO, GEO, accessibility, links).

Run from the repository root:  python3 scripts/check-site.py
"""
import json
import pathlib
import re
import sys
import xml.etree.ElementTree as ET
from html.parser import HTMLParser

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
BASE = "https://phdhebde.github.io/Lectern/"
PAGES = {"index.html": "fr", "en/index.html": "en"}
errors = []


def fail(msg):
    errors.append(msg)


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.tags = []
        self.jsonld = []
        self._in_ld = False
        self._buf = ""
        self.summaries = []
        self._in_summary = False

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        self.tags.append((tag, a))
        if tag == "script" and a.get("type") == "application/ld+json":
            self._in_ld, self._buf = True, ""
        if tag == "summary":
            self._in_summary, self._buf = True, ""

    def handle_endtag(self, tag):
        if tag == "script" and self._in_ld:
            self.jsonld.append(self._buf)
            self._in_ld = False
        if tag == "summary" and self._in_summary:
            self.summaries.append(self._buf.strip())
            self._in_summary = False

    def handle_data(self, data):
        if self._in_ld or self._in_summary:
            self._buf += data


for rel, lang in PAGES.items():
    path = SITE / rel
    html = path.read_text(encoding="utf-8")
    p = Page()
    p.feed(html)
    tags = p.tags

    def attr(tag, **match):
        return [a for t, a in tags if t == tag and all(a.get(k) == v for k, v in match.items())]

    title = re.search(r"<title>(.*?)</title>", html, re.S).group(1)
    if not 30 <= len(title) <= 70:
        fail(f"{rel}: title length {len(title)} (30-70)")
    desc = attr("meta", name="description")[0]["content"]
    if not 110 <= len(desc) <= 160:
        fail(f"{rel}: description length {len(desc)} (110-160)")
    if f'<html lang="{lang}">' not in html:
        fail(f"{rel}: html lang must be {lang}")
    if len([t for t, _ in tags if t == "h1"]) != 1:
        fail(f"{rel}: exactly one h1 expected")
    canonical = attr("link", rel="canonical")[0]["href"]
    if canonical != BASE + rel.replace("index.html", ""):
        fail(f"{rel}: canonical {canonical}")
    hreflangs = {a["hreflang"]: a["href"] for a in attr("link", rel="alternate") if "hreflang" in a}
    if set(hreflangs) != {"fr", "en", "x-default"}:
        fail(f"{rel}: hreflang set {sorted(hreflangs)}")
    for prop in ["og:title", "og:description", "og:image", "og:url"]:
        if not attr("meta", property=prop):
            fail(f"{rel}: missing {prop}")
    og_image = attr("meta", property="og:image")[0]["content"]
    if not (SITE / og_image.replace(BASE, "")).exists():
        fail(f"{rel}: og:image file missing")

    for t, a in tags:
        if t == "img":
            for k in ("alt", "width", "height"):
                if k not in a:
                    fail(f"{rel}: <img src={a.get('src')}> without {k}")
        for k in ("href", "src"):
            v = a.get(k)
            if not v or v.startswith(("http", "#", "mailto:")):
                continue
            target = (path.parent / v.split("#")[0]).resolve()
            if target.is_dir():
                target = target / "index.html"
            if not target.exists():
                fail(f"{rel}: broken local link {v}")
        if t == "a" and a.get("href", "").startswith("#") and len(a["href"]) > 1:
            if not any(b.get("id") == a["href"][1:] for _, b in tags):
                fail(f"{rel}: anchor {a['href']} has no target")

    if len(p.jsonld) != 1:
        fail(f"{rel}: one JSON-LD block expected")
    else:
        data = json.loads(p.jsonld[0])
        types = {node["@type"] for node in data["@graph"]}
        for needed in ["WebSite", "WebPage", "SoftwareApplication", "SoftwareSourceCode", "FAQPage"]:
            if needed not in types:
                fail(f"{rel}: JSON-LD lacks {needed}")
        faq = next(n for n in data["@graph"] if n["@type"] == "FAQPage")
        questions = [q["name"] for q in faq["mainEntity"]]
        if questions != p.summaries:
            fail(f"{rel}: visible FAQ and FAQPage JSON-LD differ")

ns = {"s": "http://www.sitemaps.org/schemas/sitemap/0.9"}
locs = [e.text for e in ET.parse(SITE / "sitemap.xml").getroot().findall("s:url/s:loc", ns)]
if sorted(locs) != sorted(BASE + r.replace("index.html", "") for r in PAGES):
    fail(f"sitemap locations {locs}")
robots = (SITE / "robots.txt").read_text()
if f"Sitemap: {BASE}sitemap.xml" not in robots:
    fail("robots.txt does not reference the sitemap")
for f in ["llms.txt", "llms-full.txt", "404.html", ".nojekyll"]:
    if not (SITE / f).exists():
        fail(f"missing site/{f}")

if errors:
    print("\n".join(errors))
    sys.exit(1)
print(f"site OK: {len(PAGES)} pages")
