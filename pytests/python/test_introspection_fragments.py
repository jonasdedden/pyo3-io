"""Inspect every linked protocol, including combinations unused by the test functions."""

import ctypes
import json
from typing import Any

import pytest

import pyo3_io_tests as ext

_EXTENSION_FILE = ext.__file__
assert _EXTENSION_FILE is not None, "test extension has no __file__"
LIBRARY = ctypes.PyDLL(_EXTENSION_FILE)
CAPABILITIES: tuple[tuple[str, int], ...] = (("Read", 8), ("Write", 4), ("Seek", 2), ("Fileno", 1))

#: One expected method: its positional arguments plus its return annotation.
ExpectedMethod = tuple[list[tuple[str, dict[str, Any]]], dict[str, Any]]


def fragment(alias: str, suffix: str) -> Any:
    name = f"PYO3_INTROSPECTION_1_PYO3_IO_{alias}_{suffix}"
    length = ctypes.c_uint32.in_dll(LIBRARY, name)
    # The encoder's repr(C) layout is a u32 followed immediately by JSON bytes.
    assert 0 < length.value < 16384
    data = ctypes.string_at(ctypes.addressof(length) + 4, length.value)
    return json.loads(data)


def hint(name: str) -> dict[str, Any]:
    if "." not in name:
        return {"type": "name", "id": name}
    module, attribute = name.rsplit(".", 1)
    return {"type": "attribute", "value": hint(module), "attr": attribute}


def nullable(value: dict[str, Any]) -> dict[str, Any]:
    return {
        "type": "binop", "left": value, "op": "bitor",
        "right": {"type": "constant", "kind": "none"},
    }


@pytest.mark.parametrize("kind", ["Binary", "Text"])
@pytest.mark.parametrize("bits", range(1, 16))
def test_linked_protocol_matches_its_capabilities(kind: str, bits: int) -> None:
    alias = kind + "".join(name for name, bit in CAPABILITIES if bits & bit)
    protocol = f"Supports{alias}"
    parent = f"pyo3-io:{protocol}"
    assert fragment(alias, "protocol") == {
        "type": "class",
        "attach_to_root": True,
        "name": protocol,
        "id": parent,
        "bases": [hint("typing.Protocol")],
    }

    integer = hint("int")
    methods: dict[str, ExpectedMethod] = {}
    if bits & 8:
        payload = "str" if kind == "Text" else "_typeshed.ReadableBuffer"
        methods["read"] = ([("size", integer)], nullable(hint(payload)))
    if bits & 4:
        payload = "str" if kind == "Text" else "bytes"
        methods["write"] = ([("data", hint(payload))], nullable(integer))
    if bits & 2:
        methods["seek"] = ([("offset", integer), ("whence", integer)], integer)
        if kind == "Text":
            methods["tell"] = ([], integer)
    if bits & 1:
        methods["fileno"] = ([], integer)

    for method in ("read", "write", "seek", "tell", "fileno", "flush"):
        if method not in methods:
            with pytest.raises(ValueError):
                fragment(alias, method)
            continue
        arguments, returns = methods[method]
        assert fragment(alias, method) == {
            "type": "function",
            "name": method,
            "parent": parent,
            "arguments": {
                "posonlyargs": [{"name": "self"}] + [
                    {"name": name, "annotation": annotation} for name, annotation in arguments
                ],
            },
            "returns": returns,
        }
