"""Build the Python API documentation from the installed Rust-backed package."""

import tomllib
from pathlib import Path

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
