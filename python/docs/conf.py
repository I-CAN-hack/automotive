"""Build the Python API documentation from the installed Rust-backed package."""

import tomllib
from enum import IntEnum
from importlib import import_module
from pathlib import Path

from docutils import nodes
from sphinx import addnodes
from sphinx.application import Sphinx

with (Path(__file__).resolve().parents[1] / "pyproject.toml").open("rb") as stream:
    metadata = tomllib.load(stream)["project"]

project = "Automotive for Python"
release = metadata["version"]

extensions = [
    "myst_parser",
    "sphinx.ext.autodoc",
    "sphinx.ext.githubpages",
    "sphinx.ext.napoleon",
    "sphinx.ext.viewcode",
]
autodoc_member_order = "bysource"
autodoc_typehints_format = "short"
napoleon_use_ivar = True
exclude_patterns = ["_build"]
html_theme = "furo"
html_title = project
html_baseurl = metadata["urls"]["Documentation"]
html_show_copyright = False


def format_enum_values(_app: Sphinx, doctree: nodes.document) -> None:
    """Render integer enum members as hexadecimal wire values in every output format."""
    for signature in doctree.findall(addnodes.desc_signature):
        module = signature.get("module")
        name = signature.get("fullname")
        if not module or not name:
            continue
        value: object = import_module(module)
        for part in name.split("."):
            value = getattr(value, part, None)
        if isinstance(value, IntEnum):
            for annotation in signature.findall(addnodes.desc_annotation):
                annotation.replace_self(addnodes.desc_annotation("", f" = 0x{value.value:02X}"))


def setup(app: Sphinx) -> None:
    """Register hexadecimal enum formatting for the generated documentation."""
    app.connect("doctree-read", format_enum_values)
