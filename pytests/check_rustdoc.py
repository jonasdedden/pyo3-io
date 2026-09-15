#!/usr/bin/env python3
"""Build and check the public rustdoc surface using only the Python stdlib.

Run from any directory with ``python3 pytests/check_rustdoc.py``. Both default
and all-feature docs are built for rustc's host in a disposable target directory;
neither pre-existing docs nor a configured cross-compilation target are used.
"""

import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
from html.parser import HTMLParser
from urllib.parse import unquote, urljoin, urlsplit

ROOT = Path(__file__).resolve().parents[1]
CRATE = "pyo3_typed_io"
FAMILIES = {"PyBinaryIO", "PyTextIO", "BoundPyBinaryIO", "BoundPyTextIO"}
COMMON = {
    "PyBinaryRead", "PyBinaryWrite", "PyBinaryReadSeek",
    "PyTextRead", "PyTextWrite", "PyTextReadSeek",
}
CONSTRUCTORS = {"new", "py_new"}
OWNED_COMMON = CONSTRUCTORS | {"bind", "into_bound", "as_py_object", "into_py_object"}
BOUND_COMMON = {"as_py_object", "unbind"}
TEXT = {
    "Read": {"read_chars", "read_to_string"},
    "Write": {"write_str", "write_all_str", "flush"},
    "Seek": {"tell", "seek_to", "rewind", "seek_to_end"},
}
BINARY_REQUIRED = {
    "Read": {"read", "read_exact", "read_to_end", "read_to_string"},
    "Write": {"write", "write_all", "flush"},
    "Seek": {"seek", "rewind", "stream_position"},
}


def require(condition: object, message: str) -> None:
    if not condition:
        raise AssertionError(message)


class Page(HTMLParser):
    """Retain semantic headings, links and IDs, not rustdoc's CSS or markup."""

    path: Path
    ids: set[str]
    links: list[tuple[str, str | None]]
    alias_links: list[str]
    in_term: bool
    main: bool
    section: str
    heading: tuple[int, list[str]] | None
    headings: set[str]
    redirect: str | None

    def __init__(self, path: Path) -> None:
        super().__init__(convert_charrefs=True)
        self.path = path
        self.ids = set()
        self.links = []
        self.alias_links = []
        self.in_term = False
        self.main = False
        self.section = ""
        self.heading = None
        self.headings = set()
        self.redirect = None
        self.feed(path.read_text(encoding="utf-8"))
        self.close()

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        # `dict` keeps the last value for duplicate attributes, matching browser behavior.
        attributes = dict(attrs)
        if "id" in attributes:
            # Rustdoc also percent-encodes some literal impl IDs in the HTML.
            # A valueless `id` attribute parses as None; there is nothing to record then.
            element_id = attributes["id"]
            assert element_id is not None
            self.ids.add(unquote(element_id))
        if tag == "main":
            self.main = True
        if tag == "dt":
            self.in_term = True
        if tag == "meta" and (attributes.get("http-equiv") or "").lower() == "refresh":
            match = re.search(r"url\s*=\s*(.+)", attributes.get("content") or "", re.I)
            if match:
                self.redirect = match.group(1).strip().strip("'\"")
        if self.main and re.fullmatch(r"h[1-6]", tag):
            self.heading = (int(tag[1]), [])
        if tag == "a" and "href" in attributes:
            href = attributes["href"]
            assert href is not None, "valueless href attribute"
            self.links.append((href, self.section if self.main else None))
            # Only catalog entries, not aliases mentioned in their descriptions.
            if self.main and self.in_term and self.section == "Type Aliases":
                self.alias_links.append(href)

    def handle_data(self, data: str) -> None:
        if self.heading is not None:
            self.heading[1].append(data)

    def handle_endtag(self, tag: str) -> None:
        if re.fullmatch(r"h[1-6]", tag) and self.heading is not None:
            level, parts = self.heading
            title = " ".join("".join(parts).replace("§", "").split())
            self.headings.add(title)
            if level <= 2:
                self.section = title
            self.heading = None
        if tag == "main":
            self.main = False
        if tag == "dt":
            self.in_term = False


class Docs:
    root: Path
    pages: dict[Path, Page]

    def __init__(self, root: Path) -> None:
        self.root = root.resolve()
        self.pages = {}

    def page(self, path: Path) -> Page:
        path = path.resolve()
        require(path.is_file(), f"Missing rustdoc page: {path}")
        if path not in self.pages:
            self.pages[path] = Page(path)
        return self.pages[path]

    def resolve(self, source: Path, href: str) -> tuple[Page, str] | None:
        """Follow local rustdoc redirects, preserving URL-decoded fragments."""
        url = urlsplit(urljoin(source.as_uri(), href))
        if url.scheme != "file":
            return None
        path = Path(unquote(url.path)).resolve()
        if not path.is_relative_to(self.root) or path.suffix != ".html":
            return None  # Dependencies, source listings and rustdoc assets.
        fragment = unquote(url.fragment)
        visited: set[Path] = set()
        while True:
            require(path not in visited, f"Rustdoc redirect cycle: {source} -> {href}")
            visited.add(path)
            page = self.page(path)
            if not page.redirect:
                return page, fragment
            target = urlsplit(urljoin(path.as_uri(), page.redirect))
            require(target.scheme == "file", f"Nonlocal redirect in {path}")
            path = Path(unquote(target.path)).resolve()
            require(path.is_relative_to(self.root), f"Redirect leaves crate: {path}")
            fragment = unquote(target.fragment) or fragment

    def validate_links(self) -> None:
        for path in sorted(self.root.rglob("*.html")):
            page = self.page(path)
            for href, _ in page.links:
                target = self.resolve(path, href)
                if target is not None:
                    destination, fragment = target
                    require(
                        not fragment or fragment in destination.ids,
                        f"Broken rustdoc fragment: {path.relative_to(self.root)} "
                        f"-> {href} (resolved to {destination.path.name})",
                    )
            if page.redirect:
                self.resolve(path, page.redirect)

    def aliases(self, relative: str) -> set[str]:
        page = self.page(self.root / relative)
        require("Type Aliases" in page.headings, f"No Type Aliases section: {relative}")
        return {
            unquote(urlsplit(href).path).rsplit("/", 1)[-1][5:-5]
            for href in page.alias_links
            if re.search(r"(?:^|/)type\.[^/]+\.html$", urlsplit(href).path)
        }

    def methods(self, relative: str, expected: set[str]) -> Page:
        page = self.page(self.root / relative)
        for method in sorted(expected):
            require(f"method.{method}" in page.ids, f"{relative} is missing method.{method}")
        return page


def catalog_names(bound: bool = False) -> set[str]:
    # Deliberately independent of the Rust macro table and any private generator.
    return {
        ("BoundPy" if bound else "Py") + kind
        + "".join(
            cap for bit, cap in enumerate(("Read", "Write", "Seek", "Fileno"))
            if mask & (1 << bit)
        )
        for kind in ("Binary", "Text")
        for mask in range(1, 16)
    }


def check_summary(docs: Docs, name: str, capabilities: tuple[str, ...]) -> None:
    bound = name.startswith("Bound")
    kind = "Text" if "Text" in name else "Binary"
    subdir = docs.root / "aliases" / ("bound" if bound else "")
    # Canonical catalog pages; the six common aliases are also inlined at the root.
    path = subdir / f"type.{name}.html"
    page = docs.page(path)
    require("Available operations" in page.headings, f"{name}: missing Available operations heading")
    operations: list[tuple[str, str]] = []
    for href, section in page.links:
        if section == "Available operations":
            target = docs.resolve(path, href)
            if target and target[1].startswith("method."):
                operations.append((target[0].path.name, target[1][7:]))
    common = BOUND_COMMON if bound else OWNED_COMMON
    family = f"type.{'BoundPy' if bound else 'Py'}{kind}IO.html"
    shared = f"struct.{'BoundPyIO' if bound else 'PyIO'}.html"
    groups = TEXT if kind == "Text" else BINARY_REQUIRED
    allowed = set(common) | {"clone_ref"}
    required = set(common)
    for capability in capabilities:
        allowed |= groups[capability]
        required |= groups[capability]
    methods = {method for _, method in operations}
    require(required <= methods, f"{name}: missing summary methods {required - methods}")
    require(methods <= allowed, f"{name}: unrequested summary methods {methods - allowed}")
    for target_name, method in operations:
        expected = shared if method in common else family
        require(target_name == expected, f"{name}: {method} links to {target_name}, expected {expected}")
    if bound:
        require(not (CONSTRUCTORS & methods), f"{name}: bound alias claims constructors")


def check_docs(root: Path, unix: bool) -> None:
    docs = Docs(root)
    for relative, expected in (
        ("index.html", COMMON | FAMILIES),
        ("aliases/index.html", catalog_names()),
        ("aliases/bound/index.html", catalog_names(bound=True)),
    ):
        actual = docs.aliases(relative)
        require(
            actual == expected,
            f"{relative}: missing aliases {expected - actual}; "
            f"unexpected aliases {actual - expected}",
        )
    docs.methods("struct.PyIO.html", OWNED_COMMON | {"fileno"})
    docs.methods("struct.BoundPyIO.html", BOUND_COMMON | {"fileno"})
    for name in sorted(FAMILIES):
        binary = "Binary" in name
        expected = (
            set().union(*BINARY_REQUIRED.values()) if binary else set().union(*TEXT.values())
        )
        page = docs.methods(f"type.{name}.html", expected)
        for trait in ("Read", "Write", "Seek"):
            present = any(anchor.startswith(f"impl-{trait}-for-") for anchor in page.ids)
            require(
                present == binary,
                f"{name}: std::io::{trait} impl {'missing' if binary else 'unexpected'}",
            )
    for name, capabilities in (
        ("PyBinaryRead", ("Read",)),
        ("PyBinaryReadSeek", ("Read", "Seek")),
        ("PyTextWrite", ("Write",)),
        ("PyTextReadSeek", ("Read", "Seek")),
        ("BoundPyBinaryRead", ("Read",)),
        ("BoundPyTextReadSeek", ("Read", "Seek")),
    ):
        check_summary(docs, name, capabilities)
    # Descriptor operations belong to the shared type, not a payload family.
    for bound in (False, True):
        name = ("BoundPy" if bound else "Py") + "BinaryFileno"
        subdir = root / "aliases" / ("bound" if bound else "")
        path = subdir / f"type.{name}.html"
        page = docs.page(path)
        shared = f"struct.{'BoundPyIO' if bound else 'PyIO'}.html"
        links = {
            (target[0].path.name, target[1])
            for href, section in page.links
            if section == "Available operations"
            for target in [docs.resolve(path, href)]
            if target is not None
        }
        require((shared, "method.fileno") in links, f"{name}: missing shared fileno link")
        clone_links = {(target, fragment) for target, fragment in links if fragment == "method.try_clone_fd"}
        require(
            clone_links == ({(shared, "method.try_clone_fd")} if unix else set()),
            f"{name}: incorrect Unix-only try_clone_fd links: {clone_links}",
        )
    docs.validate_links()


def main() -> None:
    version = subprocess.check_output(["rustc", "-vV"], cwd=ROOT, text=True)
    host = next(
        (line.removeprefix("host: ") for line in version.splitlines() if line.startswith("host: ")),
        None,
    )
    require(host, "rustc -vV did not report a host target")
    # `require` raises, but the checkers cannot see that; spell out the narrowing.
    assert host is not None
    cfg = subprocess.check_output(["rustc", "--print", "cfg", "--target", host], cwd=ROOT, text=True)
    unix = "unix" in cfg.splitlines()
    target_parent = ROOT / "target"
    target_parent.mkdir(exist_ok=True)
    env = os.environ.copy()
    deny = ["-D", "rustdoc::broken_intra_doc_links", "-D", "rustdoc::private_intra_doc_links"]
    # Cargo's encoded flags take precedence over both RUSTDOCFLAGS and config.
    flags = (
        env["CARGO_ENCODED_RUSTDOCFLAGS"].split("\x1f")
        if env.get("CARGO_ENCODED_RUSTDOCFLAGS")
        else shlex.split(env.get("RUSTDOCFLAGS", ""))
    )
    env["CARGO_ENCODED_RUSTDOCFLAGS"] = "\x1f".join(flags + deny)
    with tempfile.TemporaryDirectory(prefix="rustdoc-check-", dir=target_parent) as temp:
        target = Path(temp)
        doc_tree = target / host / "doc"
        for all_features in (False, True):
            # Preserve dependency compilation, but never accept HTML from the last pass.
            if doc_tree.exists():
                shutil.rmtree(doc_tree)
            label = "all features" if all_features else "default features"
            print(f"Checking rustdoc ({label}, host {host})", flush=True)
            command = [
                "cargo", "doc", "--locked", "--no-deps",
                "--manifest-path", str(ROOT / "Cargo.toml"),
                "--target", host, "--target-dir", str(target),
            ]
            if all_features:
                command.append("--all-features")
            subprocess.run(command, cwd=ROOT, env=env, check=True)
            check_docs(doc_tree / CRATE, unix)
    print("Rendered rustdoc checks passed (default and all features).")


if __name__ == "__main__":
    main()
