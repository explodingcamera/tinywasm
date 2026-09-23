"""Check every non-inline function declared in the vendored ABI is exported."""
import ctypes
from pathlib import Path
import re
import shlex
import subprocess
import sys

header = subprocess.check_output(
    [*shlex.split(sys.argv[2]), "-E", "-P", "-DWASM_API_EXTERN=ABI_SYMBOL", "-x", "c", "include/tinywasm.h"],
    text=True,
)
symbols = set(re.findall(r"ABI_SYMBOL\s+[^;{}]*?\b((?:wasm|tinywasm)_\w+)\s*\(", header))
assert len(symbols) > 200, "header preprocessing did not find the API"
library = ctypes.CDLL(sys.argv[1])
prefix = sys.argv[3] if len(sys.argv) > 3 else ""
missing = sorted(symbol for symbol in symbols if not hasattr(library, prefix + symbol))
assert not missing, f"Missing symbols: {missing}"
aliases = set(re.findall(r"^#define ((?:wasm|tinywasm)_\w+) TINYWASM_SYMBOL", Path("include/tinywasm-prefix.h").read_text(), re.M))
assert aliases == symbols, f"Prefix aliases differ: {aliases ^ symbols}"
if prefix:
    assert not any(hasattr(library, symbol) for symbol in symbols), "unprefixed API symbols leaked"
print(f"Verified {len(symbols)} exported API symbols")
