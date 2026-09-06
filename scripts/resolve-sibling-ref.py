#!/usr/bin/env python3
"""Resolve sibling repositories at the current branch before falling back.

The route-map gate compares contracts across sibling repositories. A coordinated
change should compare matching branch names first; falling back to ``main`` is
only correct when the sibling branch does not exist. This resolver makes that
choice explicit, emits the selected ref through ``GITHUB_OUTPUT``, and records
resolution evidence that can be printed alongside a later route-check failure.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Callable, NamedTuple, TextIO
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen

GITHUB_API_VERSION = "2022-11-28"
REPOSITORY_RE = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\Z")


class ResolutionError(RuntimeError):
    """The requested sibling revision could not be resolved safely."""


class Resolution(NamedTuple):
    ref: str
    source: str
    matching_status: int
    fallback_status: int | None


StatusLookup = Callable[[str, str, str | None, str], int]


def validate_repository(repository: str) -> str:
    if not REPOSITORY_RE.fullmatch(repository):
        raise ResolutionError(
            f"invalid repository {repository!r}; expected an owner/repository slug"
        )
    return repository


def branch_url(api_url: str, repository: str, ref: str) -> str:
    repository = validate_repository(repository)
    if not ref:
        raise ResolutionError("branch ref is empty")
    encoded_ref = quote(ref, safe="")
    return f"{api_url.rstrip('/')}/repos/{repository}/branches/{encoded_ref}"


def branch_status(
    repository: str,
    ref: str,
    token: str | None,
    api_url: str,
) -> int:
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "ORESoftware-api-docs-route-sync",
        "X-GitHub-Api-Version": GITHUB_API_VERSION,
    }
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = Request(branch_url(api_url, repository, ref), headers=headers)
    try:
        with urlopen(request, timeout=20) as response:
            return int(response.status)
    except HTTPError as exc:
        return int(exc.code)
    except URLError as exc:
        raise ResolutionError(
            f"GitHub branch lookup failed for {repository}@{ref}: {exc.reason}"
        ) from exc


def resolve_ref(
    repository: str,
    matching_ref: str,
    *,
    fallback_ref: str = "main",
    token: str | None = None,
    api_url: str = "https://api.github.com",
    status_lookup: StatusLookup = branch_status,
) -> Resolution:
    repository = validate_repository(repository)
    if not matching_ref:
        raise ResolutionError("matching branch ref is empty")
    if not fallback_ref:
        raise ResolutionError("fallback branch ref is empty")

    matching_status = status_lookup(repository, matching_ref, token, api_url)
    if matching_status == 200:
        return Resolution(matching_ref, "matching", matching_status, None)
    if matching_status != 404:
        raise ResolutionError(
            f"GitHub returned HTTP {matching_status} while checking "
            f"{repository}@{matching_ref}; refusing to disguise an access or API "
            "failure as a missing branch"
        )

    if matching_ref == fallback_ref:
        raise ResolutionError(
            f"{repository}@{matching_ref} does not exist or is not readable"
        )

    fallback_status = status_lookup(repository, fallback_ref, token, api_url)
    if fallback_status == 200:
        return Resolution(fallback_ref, "fallback", matching_status, fallback_status)
    if fallback_status == 404:
        raise ResolutionError(
            f"neither {repository}@{matching_ref} nor {repository}@{fallback_ref} "
            "exists or is readable by this workflow token"
        )
    raise ResolutionError(
        f"GitHub returned HTTP {fallback_status} while checking fallback "
        f"{repository}@{fallback_ref}; refusing to continue"
    )


def workflow_escape(value: str) -> str:
    return value.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def single_line(value: object) -> str:
    return str(value).replace("\r", " ").replace("\n", " ")


def write_resolution_log(
    path: Path,
    repository: str,
    matching_ref: str,
    fallback_ref: str,
    resolution: Resolution,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    record = {
        "fallback_ref": fallback_ref,
        "matching_ref": matching_ref,
        "repository": repository,
        "resolved_ref": resolution.ref,
        "source": resolution.source,
    }
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")


def resolution_context_lines(path: Path) -> list[str]:
    if not path.is_file():
        return [f"route-map-sync sibling context unavailable: {path} does not exist"]

    messages: list[str] = []
    for number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw_line.strip():
            continue
        try:
            record = json.loads(raw_line)
        except json.JSONDecodeError as exc:
            messages.append(
                f"route-map-sync sibling context line {number} is invalid JSON: {exc.msg}"
            )
            continue

        repository = single_line(record.get("repository", "<unknown repository>"))
        matching_ref = single_line(record.get("matching_ref", "<unknown branch>"))
        resolved_ref = single_line(record.get("resolved_ref", "<unknown ref>"))
        source = record.get("source")
        if source == "fallback":
            messages.append(
                "route-map-sync sibling context: "
                f"{repository} had no readable matching branch {matching_ref}; "
                f"the check used fallback {resolved_ref}. A route mismatch may mean "
                "the sibling branch or PR has not been pushed or merged."
            )
        elif source == "matching":
            messages.append(
                "route-map-sync sibling context: "
                f"{repository} was checked at matching branch {resolved_ref}."
            )
        else:
            messages.append(
                "route-map-sync sibling context: "
                f"{repository} recorded unknown resolution source {single_line(source)!r}."
            )
    if not messages:
        messages.append(f"route-map-sync sibling context unavailable: {path} is empty")
    return messages


def explain_resolution_log(path: Path, *, stream: TextIO | None = None) -> int:
    target = sys.stderr if stream is None else stream
    for message in resolution_context_lines(path):
        print(message, file=target)
    return 0


def emit_resolution(
    repository: str,
    matching_ref: str,
    fallback_ref: str,
    resolution: Resolution,
    *,
    output_path: str | None,
    resolution_log: Path | None,
) -> None:
    if resolution.source == "matching":
        print(
            "::notice title=Sibling route-map source::"
            + workflow_escape(
                f"{repository} resolved at matching branch {resolution.ref}"
            )
        )
    else:
        print(
            "::warning title=Sibling route-map fallback::"
            + workflow_escape(
                f"{repository} has no readable matching branch {matching_ref}; "
                f"using {fallback_ref}. A route mismatch may mean the sibling PR "
                "has not been pushed or merged."
            )
        )

    if resolution_log is not None:
        write_resolution_log(
            resolution_log,
            repository,
            matching_ref,
            fallback_ref,
            resolution,
        )

    lines = f"ref={resolution.ref}\nsource={resolution.source}\n"
    if output_path:
        with Path(output_path).open("a", encoding="utf-8") as handle:
            handle.write(lines)
    else:
        print(lines, end="")


def run(
    argv: list[str] | None = None,
    *,
    environ: dict[str, str] | None = None,
    status_lookup: StatusLookup = branch_status,
) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository")
    parser.add_argument("--matching-ref")
    parser.add_argument("--fallback-ref", default="main")
    parser.add_argument("--resolution-log", type=Path)
    parser.add_argument(
        "--explain-log",
        type=Path,
        help="print recorded sibling resolution context for a failed route check",
    )
    args = parser.parse_args(argv)

    if args.explain_log is not None:
        if args.repository or args.matching_ref or args.resolution_log:
            print(
                "--explain-log cannot be combined with resolution arguments",
                file=sys.stderr,
            )
            return 2
        return explain_resolution_log(args.explain_log)

    if not args.repository or not args.matching_ref:
        print(
            "--repository and --matching-ref are required when resolving a sibling",
            file=sys.stderr,
        )
        return 2

    env = os.environ if environ is None else environ
    token = env.get("GITHUB_TOKEN") or env.get("GH_TOKEN")
    api_url = env.get("GITHUB_API_URL", "https://api.github.com")
    try:
        resolution = resolve_ref(
            args.repository,
            args.matching_ref,
            fallback_ref=args.fallback_ref,
            token=token,
            api_url=api_url,
            status_lookup=status_lookup,
        )
        emit_resolution(
            args.repository,
            args.matching_ref,
            args.fallback_ref,
            resolution,
            output_path=env.get("GITHUB_OUTPUT"),
            resolution_log=args.resolution_log,
        )
    except (OSError, ResolutionError) as exc:
        print(
            "::error title=Sibling route-map resolution::"
            + workflow_escape(str(exc)),
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(run())
