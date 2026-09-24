#!/usr/bin/env python3
"""Translate host compilation paths and select Graphit's allowed source units."""

import json
import ntpath
import os
import posixpath
import re
import sys


def is_windows_path(value):
    return bool(re.match(r"^[A-Za-z]:[\\/]", value)) or value.startswith("\\\\")


def workspace_path(value, host_root):
    if not isinstance(value, str):
        return None
    normalized = value.replace("\\", "/") if is_windows_path(host_root) else value
    if normalized == "/workspace" or normalized.startswith("/workspace/"):
        result = posixpath.normpath(normalized)
        return result if result == "/workspace" or result.startswith("/workspace/") else None
    if is_windows_path(host_root):
        if not is_windows_path(value):
            return None
        root = ntpath.normpath(host_root)
        candidate = ntpath.normpath(value)
        try:
            if ntpath.commonpath([ntpath.normcase(root), ntpath.normcase(candidate)]) != ntpath.normcase(root):
                return None
            rel = ntpath.relpath(candidate, root).replace("\\", "/")
        except ValueError:
            return None
    else:
        if not posixpath.isabs(value):
            return None
        root = posixpath.normpath(host_root)
        candidate = posixpath.normpath(value)
        if posixpath.commonpath([root, candidate]) != root:
            return None
        rel = posixpath.relpath(candidate, root)
    return "/workspace" if rel == "." else posixpath.join("/workspace", rel)


def rewrite_embedded(value, host_root, single_arg=False):
    if not isinstance(value, str):
        return value
    variants = {host_root}
    if is_windows_path(host_root):
        variants.update({host_root.replace("\\", "/"), host_root.replace("/", "\\")})
    else:
        variants.add(host_root.replace(" ", "\\ "))
    for prefix in sorted(variants, key=len, reverse=True):
        pattern = re.compile(re.escape(prefix) + r"(?=$|[\\/\s\"'])",
                             re.IGNORECASE if is_windows_path(host_root) else 0)
        parts = []
        cursor = 0
        for match in pattern.finditer(value):
            if match.start() < cursor:
                continue
            before = value[:match.start()]
            joined_flag = re.search(r"(?:^|\s)-[A-Za-z][A-Za-z0-9_-]*$", before)
            if before and before[-1] not in " \t\n\r\"'=,:" and not joined_flag:
                continue
            quote = value[match.start() - 1] if match.start() > 0 and value[match.start() - 1] in "\"'" else None
            end = match.end()
            while end < len(value):
                if quote and value[end] == quote:
                    break
                if not quote and not single_arg and (value[end] in "\"'" or value[end].isspace() and (end == 0 or value[end - 1] != "\\")):
                    break
                end += 1
            suffix = value[match.end():end]
            if is_windows_path(host_root):
                suffix = suffix.replace("\\", "/")
            parts.extend((value[cursor:match.start()], "/workspace", suffix))
            cursor = end
        parts.append(value[cursor:])
        value = "".join(parts)
    return value


def source_relative(command, host_root):
    source = command.get("file")
    directory = command.get("directory")
    mapped = workspace_path(source, host_root)
    if mapped is None and isinstance(source, str) and isinstance(directory, str):
        base = workspace_path(directory, host_root)
        if base is not None and not posixpath.isabs(source) and not is_windows_path(source):
            mapped = posixpath.normpath(posixpath.join(base, source.replace("\\", "/")))
    if mapped is None or not mapped.startswith("/workspace/"):
        return None
    return posixpath.relpath(mapped, "/workspace")


def canonicalize_source_reference(value, original_rel, canonical_rel, single_arg=False, windows=False):
    if not isinstance(value, str) or original_rel is None or original_rel == canonical_rel and not windows:
        return value
    for old, new in (("/workspace/" + original_rel, "/workspace/" + canonical_rel),
                     (original_rel, canonical_rel)):
        comparable = value.replace("\\", "/") if windows else value
        if single_arg and comparable.casefold() == old.casefold():
            return new
        old_pattern = re.escape(old).replace("/", r"[\\/]") if windows else re.escape(old)
        value = re.sub(r"(?<![A-Za-z0-9_./\\])" + old_pattern + r"(?=$|[\s\"'])",
                       lambda _: new, value, flags=re.IGNORECASE)
    return value


def normalize_windows_argument(value):
    if not isinstance(value, str) or "\\" not in value or is_windows_path(value):
        return value
    for flag in ("-include", "-idirafter", "-isystem", "-iquote", "-imacros", "-iframework", "-I", "-F", "-o"):
        if value.startswith(flag) and len(value) > len(flag):
            return flag + value[len(flag):].replace("\\", "/")
    if not value.startswith("-"):
        return value.replace("\\", "/")
    return value


def normalize_windows_command_paths(value):
    pattern = re.compile(r"(?<!\S)(-(?:include|idirafter|isystem|iquote|imacros|iframework|I|F|o))([^\s\"']+)")
    return pattern.sub(lambda match: match.group(1) + match.group(2).replace("\\", "/"), value)


def transform(commands, host_root, allowed):
    retained = []
    unresolved = 0
    windows = is_windows_path(host_root)
    allowed_lookup = {name.casefold(): name for name in allowed} if allowed is not None and windows else None
    for command in commands:
        rel = source_relative(command, host_root)
        original_rel = rel
        if allowed is not None and rel is not None:
            if windows:
                canonical = allowed_lookup.get(rel.casefold())
                if canonical is None:
                    continue
                rel = canonical
            elif rel not in allowed:
                continue
        if allowed is not None and rel is None:
            unresolved += 1  # Keep it; Graphit's final document filter is authoritative.
        copy = dict(command)
        relative_source = None
        relative_canonical = None
        mapped_dir = workspace_path(command.get("directory"), host_root)
        if windows and rel is not None and original_rel is not None and mapped_dir is not None:
            dir_rel = posixpath.relpath(mapped_dir, "/workspace")
            old_parts = original_rel.split("/")
            new_parts = rel.split("/")
            dir_parts = [] if dir_rel == "." else dir_rel.split("/")
            if len(dir_parts) <= len(old_parts) and [x.casefold() for x in dir_parts] == [x.casefold() for x in old_parts[:len(dir_parts)]]:
                copy["directory"] = posixpath.join("/workspace", *new_parts[:len(dir_parts)])
                relative_source = "/".join(old_parts[len(dir_parts):])
                relative_canonical = "/".join(new_parts[len(dir_parts):])
        for key in ("directory", "file"):
            if key == "file" and rel is not None:
                copy[key] = posixpath.join("/workspace", rel)
                continue
            if key == "directory" and "directory" in copy and copy[key] != command.get(key):
                continue
            mapped = workspace_path(copy.get(key), host_root)
            if mapped is not None:
                copy[key] = mapped
            elif isinstance(copy.get(key), str):
                copy[key] = rewrite_embedded(copy[key], host_root, single_arg=True)
        if isinstance(copy.get("command"), str):
            rewritten = rewrite_embedded(copy["command"], host_root)
            rewritten = canonicalize_source_reference(rewritten, original_rel, rel, windows=windows)
            rewritten = canonicalize_source_reference(rewritten, relative_source, relative_canonical, windows=windows)
            copy["command"] = normalize_windows_command_paths(rewritten) if windows else rewritten
        if isinstance(copy.get("arguments"), list):
            translated = []
            for arg in copy["arguments"]:
                arg = rewrite_embedded(arg, host_root, single_arg=True)
                arg = canonicalize_source_reference(arg, original_rel, rel, single_arg=True, windows=windows)
                arg = canonicalize_source_reference(arg, relative_source, relative_canonical, single_arg=True, windows=windows)
                translated.append(normalize_windows_argument(arg) if windows else arg)
            copy["arguments"] = translated
        retained.append(copy)
    return retained, unresolved


def main():
    source, output = sys.argv[1:3]
    host_root = os.environ["GRAPHIT_SCIP_HOST_ROOT"]
    allowlist = "/cache/graphit-allowed-files.json"
    with open(source, encoding="utf-8") as stream:
        commands = json.load(stream)
    allowed = None
    if os.path.isfile(allowlist):
        with open(allowlist, encoding="utf-8") as stream:
            allowed = set(json.load(stream))
    retained, unresolved = transform(commands, host_root, allowed)
    if unresolved:
        print(f"Graphit: retained {unresolved} unresolved compilation entries for final filtering", file=sys.stderr)
    with open(output, "w", encoding="utf-8") as stream:
        json.dump(retained, stream, separators=(",", ":"))


if __name__ == "__main__":
    main()
