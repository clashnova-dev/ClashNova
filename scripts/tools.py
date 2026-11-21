"""Locate build tools that may not be on PATH (common on Windows)."""
from __future__ import annotations

import os
import pathlib
import shutil
import subprocess


def _prepend_path(directory: pathlib.Path) -> None:
    path = str(directory)
    current = os.environ.get("PATH", "")
    parts = current.split(os.pathsep) if current else []
    if path not in parts:
        os.environ["PATH"] = path + (os.pathsep + current if current else "")


def _go_candidates() -> list[pathlib.Path]:
    home = pathlib.Path.home()
    roots: list[pathlib.Path] = []
    for value in (os.environ.get("GOROOT"), os.environ.get("GOTOOLCHAIN_ROOT")):
        if value:
            roots.append(pathlib.Path(value) / "bin")
    roots.extend(
        [
            home / "sdk" / "go1.27.4" / "go" / "bin",
            home / "sdk" / "go" / "bin",
            home / "go" / "bin",
            pathlib.Path(r"C:\Program Files\Go\bin"),
            pathlib.Path(r"C:\Go\bin"),
        ]
    )
    sdk = home / "sdk"
    if sdk.is_dir():
        for child in sorted(sdk.glob("go*/go/bin"), reverse=True):
            roots.append(child)
    return roots


def _rust_candidates() -> list[pathlib.Path]:
    home = pathlib.Path.home()
    cargo_home = os.environ.get("CARGO_HOME")
    roots = []
    if cargo_home:
        roots.append(pathlib.Path(cargo_home) / "bin")
    roots.extend(
        [
            home / ".cargo" / "bin",
            pathlib.Path(os.environ.get("USERPROFILE", str(home))) / ".cargo" / "bin",
        ]
    )
    return roots


def ensure_tool(name: str, candidates: list[pathlib.Path], version_args: list[str], hint: str) -> str:
    existing = shutil.which(name)
    if existing:
        return existing
    exe_name = f"{name}.exe" if os.name == "nt" else name
    for directory in candidates:
        exe = directory / exe_name
        if not exe.is_file():
            continue
        _prepend_path(directory)
        try:
            subprocess.check_output([str(exe), *version_args], text=True, stderr=subprocess.STDOUT)
        except (OSError, subprocess.CalledProcessError):
            continue
        return str(exe)
    raise FileNotFoundError(hint)


def ensure_go_on_path() -> str:
    return ensure_tool(
        "go",
        _go_candidates(),
        ["version"],
        "未找到 Go。请安装 Go 1.22+ 并加入 PATH，"
        "或放到 %USERPROFILE%\\sdk\\go1.27.4\\go\\bin（本仓库本地约定）。",
    )


def ensure_rustc_on_path() -> str:
    return ensure_tool(
        "rustc",
        _rust_candidates(),
        ["--version"],
        "未找到 rustc。请安装 Rust（rustup）并确保 %USERPROFILE%\\.cargo\\bin 在 PATH 中。",
    )


def go_cmd() -> list[str]:
    return [ensure_go_on_path()]


def rustc_cmd() -> list[str]:
    return [ensure_rustc_on_path()]
