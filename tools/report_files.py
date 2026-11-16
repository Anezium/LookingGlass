"""Report-file names across the renamed public convention and preserved evidence.

The public tree names reports REPORT.md and REPORT-<stage>.md. Preserved
campaign output directories predate that convention and carry a prefix before
REPORT. Resolution is done by glob and by the canonical suffix so the old
prefix never has to be written in this tree.
"""
from pathlib import Path
import shutil

MARKER = "REPORT"


def canonical_report_name(name):
    """Return REPORT<stage>.md for a canonical or prefixed report name, else None."""
    base = Path(name).name
    if not base.endswith(".md"):
        return None
    index = base.upper().find(MARKER)
    if index < 0 or (index > 0 and base[index - 1] != "-"):
        return None
    rest = base[index + len(MARKER):]
    if rest == ".md":
        return MARKER + ".md"
    if rest.startswith("-") and rest.endswith(".md") and len(rest) > len("-.md"):
        return MARKER + rest
    return None


def report_suffix(name):
    """Return '' for REPORT.md, '-R11D' for REPORT-R11D.md/prefixed equivalents."""
    canonical = canonical_report_name(name)
    return None if canonical is None else canonical[len(MARKER):-len(".md")]


def is_report_file(name, suffix=None):
    """True for any report name, or only the report with the requested suffix."""
    actual = report_suffix(name)
    return actual is not None and (suffix is None or actual == suffix)


def report_path(directory, suffix=""):
    """Resolve a report in a directory: canonical first, then one prefixed file."""
    directory = Path(directory)
    canonical = directory / f"{MARKER}{suffix}.md"
    if canonical.is_file():
        return canonical
    candidates = [p for p in directory.glob(f"*{MARKER}{suffix}.md")
                  if p.is_file() and report_suffix(p.name) == suffix]
    if len(candidates) > 1:
        raise ValueError(f"ambiguous report files for suffix {suffix!r} in {directory}")
    return candidates[0] if candidates else canonical


def resolve(directory, name):
    """Resolve a canonical report name against a directory that may hold the old prefix."""
    if is_report_file(name):
        return report_path(directory, report_suffix(name))
    return Path(directory) / name


def copy_documents(old, new):
    """Copy top-level Markdown, renaming report files to the canonical name.

    Returns the (source name, destination name) pairs so callers and tests can
    verify that no prefixed report name is created in the new directory.
    """
    old, new = Path(old), Path(new)
    copied = []
    for source in sorted(old.glob("*.md")):
        target_name = canonical_report_name(source.name) or source.name
        target = new / target_name
        if target.exists():
            raise ValueError(f"duplicate document target: {target}")
        shutil.copyfile(source, target)
        copied.append((source.name, target_name))
    return copied
