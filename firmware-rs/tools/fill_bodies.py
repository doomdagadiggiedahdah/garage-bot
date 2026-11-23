#!/usr/bin/env python3
"""Fill every logic/src/fns/NAME.rs body mechanically (all 69 functions are "copy":
the contract is `result == spec_x(args)` and spec_x is executable code).

Source of each body, in order:
  1. --haiku FN=BRANCH: the body region of a fleet-1 Haiku branch (git show).
  2. otherwise "script": the text of the specification, with
       - spec_c renamed to c for every listed callee c (as the reference bodies were),
       - every other spec.rs item it needs (helper fns, consts) copied in as a nested
         item named h_<name> / H_<NAME>, so the body never touches spec.rs
         (spec.rs is not compiled into the firmware).

    fill_bodies.py [--only a,b] [--haiku fn=branch ...] [--report out.json]
Only a stub body (`unimplemented!(... body not written yet)`) is replaced unless --force.
"""
import json, os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "logic", "src")
FNS = os.path.join(SRC, "fns")


# ------------------------------------------------------------ a small Rust lexer
def lex(s):
    """Yield (kind, text) with kind in id, str, chr, cmt, ws, p (punct)."""
    i, n = 0, len(s)
    while i < n:
        c = s[i]
        if s.startswith("//", i):
            j = s.find("\n", i); j = n if j < 0 else j
            yield "cmt", s[i:j]; i = j
        elif s.startswith("/*", i):
            j = s.find("*/", i + 2) + 2
            yield "cmt", s[i:j]; i = j
        elif re.match(r'b?r#*"', s[i:i + 4]) and (i == 0 or not (s[i - 1].isalnum() or s[i - 1] == "_")):
            m = re.match(r'(b?r)(#*)"', s[i:])
            end = '"' + m.group(2)
            j = s.find(end, i + len(m.group(0))) + len(end)
            yield "str", s[i:j]; i = j
        elif c == '"' or (c == "b" and i + 1 < n and s[i + 1] == '"' and (i == 0 or not (s[i - 1].isalnum() or s[i - 1] == "_"))):
            j = i + (2 if c == "b" else 1)
            while s[j] != '"':
                j += 2 if s[j] == "\\" else 1
            yield "str", s[i:j + 1]; i = j + 1
        elif c == "'" or (c == "b" and i + 1 < n and s[i + 1] == "'" and (i == 0 or not (s[i - 1].isalnum() or s[i - 1] == "_"))):
            k = i + (2 if c == "b" else 1)
            m = re.match(r"(\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'", s[k:])
            if m:
                yield "chr", s[i:k + len(m.group(0))]; i = k + len(m.group(0))
            else:  # lifetime
                m = re.match(r"'[A-Za-z_]\w*", s[i:])
                yield "p", m.group(0); i += len(m.group(0))
        elif c.isalpha() or c == "_":
            m = re.match(r"[A-Za-z_]\w*", s[i:])
            yield "id", m.group(0); i += len(m.group(0))
        elif c.isspace():
            m = re.match(r"\s+", s[i:])
            yield "ws", m.group(0); i += len(m.group(0))
        else:
            yield "p", c; i += 1


def match_brace(s, start):
    """Index just past the brace/paren that closes the one at s[start]."""
    open_c = s[start]; close_c = {"{": "}", "(": ")", "[": "]"}[open_c]
    depth, pos = 0, 0
    for kind, t in lex(s[start:]):
        if kind == "p" and t == open_c:
            depth += 1
        elif kind == "p" and t == close_c:
            depth -= 1
            if depth == 0:
                return start + pos + 1
        pos += len(t)
    raise ValueError("unbalanced")


# ------------------------------------------------------------ spec.rs items
def spec_items():
    text = open(os.path.join(SRC, "spec.rs")).read()
    cut = text.find("#[cfg(kani)]\n#[allow(static_mut_refs)]")
    text = text[:cut] if cut > 0 else text
    items = {}
    for m in re.finditer(r"^(pub )?(fn|const) (\w+)", text, re.M):
        kind, name, a = m.group(2), m.group(3), m.start()
        if kind == "fn":
            p0 = text.index("(", m.end())
            p1 = match_brace(text, p0)
            b0 = text.index("{", p1)
            b1 = match_brace(text, b0)
            items[name] = dict(kind="fn", params=split_params(text[p0 + 1:p1 - 1]),
                               ret=text[p1:b0].strip(), body=text[b0 + 1:b1 - 1], full=text[a:b1])
        else:
            j = a
            for kind2, t in lex(text[a:]):
                j += len(t)
                if kind2 == "p" and t == ";":
                    break
            items[name] = dict(kind="const", full=text[a:j])
    return items


def split_params(p):
    out, depth, cur = [], 0, ""
    for ch in p:
        if ch in "(<[":
            depth += 1
        elif ch in ")>]":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur); cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur)
    res = []
    for x in out:
        nm, ty = x.split(":", 1)
        res.append((nm.strip().replace("mut ", ""), ty.strip()))
    return res


def ids(text):
    return {t for k, t in lex(text) if k == "id"}


def rename(text, mp):
    out, prev = [], ""
    for k, t in lex(text):
        if k == "id" and t in mp and prev != ".":
            t = mp[t]
        out.append(t)
        if k != "ws":
            prev = t
    return "".join(out)


def fix_pad6(text):
    """spec_text_push_padded(a, b, 6) -> text_push_pad6(a, b)."""
    key = "spec_text_push_padded("
    while key in text:
        i = text.index(key)
        p0 = i + len(key) - 1
        p1 = match_brace(text, p0)
        args = split_args(text[p0 + 1:p1 - 1])
        assert len(args) == 3 and args[2].strip() == "6", args
        text = text[:i] + f"text_push_pad6({args[0].strip()}, {args[1].strip()})" + text[p1:]
    return text


def split_args(a):
    out, depth, cur = [], 0, ""
    for k, t in lex(a):
        if k == "p" and t in "([{":
            depth += 1
        elif k == "p" and t in ")]}":
            depth -= 1
        if k == "p" and t == "," and depth == 0:
            out.append(cur); cur = ""
        else:
            cur += t
    out.append(cur)
    return out


def helper_name(n):
    if n.isupper():
        return "H_" + n
    return "h_" + (n[5:] if n.startswith("spec_") else n)


def script_body(fn, funcs, items):
    f = funcs[fn]
    spec_call = f["spec"]
    spec_name = spec_call.split("(")[0]
    s = items[spec_name]
    callee_map = {}
    for c in f["callees"]:
        sp = funcs[c]["spec"].split("(")[0]
        if "(" not in funcs[c]["spec"]:
            callee_map[sp] = c
    pad6_callee = "text_push_pad6" in f["callees"]

    def xform(text):
        if pad6_callee:
            text = fix_pad6(text)
        return text

    # helpers needed, transitively
    need, todo, seen = [], [xform(s["body"])], set()
    while todo:
        t = todo.pop()
        for i in sorted(ids(t)):
            if i in items and i not in callee_map and i != spec_name and i not in seen:
                if i == "spec_text_push_padded" and pad6_callee:
                    continue
                seen.add(i); need.append(i)
                todo.append(xform(items[i]["full"]))
    mp = dict(callee_map)
    for i in need:
        mp[i] = helper_name(i)

    lines = []
    # parameters: the fn's own names vs the spec's
    fparams = fn_params(fn)
    sparams = s["params"]
    for idx, (sn, sty) in enumerate(sparams):
        if idx < len(fparams):
            fnm = fparams[idx][0]
            if fnm != sn:
                lines.append(f"let {sn} = {fnm};")
        else:  # extra spec argument fixed by the contract, e.g. width 6 for text_push_pad6
            val = split_args(spec_call[spec_call.index("(") + 1:-1])[idx].strip()
            lines.append(f"let {sn}: {sty} = {val};")
    for i in need:
        it = rename(xform(items[i]["full"]), mp)
        it = re.sub(r"^pub ", "", it)
        if "'static" in it:  # the purity lint refuses the word `static`; a named lifetime is the same here
            it = it.replace("'static", "'a")
            it = re.sub(r"^fn (\w+)\(", r"fn \1<'a>(", it)
        lines.append(it)
    body = rename(xform(s["body"]), mp)
    out = []
    for l in lines:
        out += ["    " + x if x.strip() else "" for x in l.split("\n")]
    blines = body.split("\n")
    while blines and not blines[0].strip():
        blines.pop(0)
    while blines and not blines[-1].strip():
        blines.pop()
    out += blines
    return out


def fn_params(fn):
    t = open(os.path.join(FNS, fn + ".rs")).read()
    m = re.search(r"pub fn %s\(" % fn, t)
    p0 = m.end() - 1
    return split_params(t[p0 + 1:match_brace(t, p0) - 1])


def body_region(lines):
    b = next(i for i, l in enumerate(lines) if l.strip() == "// @body")
    e = next(i for i in range(b + 1, len(lines)) if lines[i].strip() == "// @end")
    assert lines[e - 1].rstrip() == "}", "body must end with } before // @end"
    return b + 1, e - 1


def main():
    args = sys.argv[1:]
    funcs = {f["name"]: f for f in json.load(open(os.path.join(ROOT, "FUNCTIONS.json")))}
    haiku = {}
    for a in args:
        if "=" in a and not a.startswith("--"):
            k, v = a.split("=", 1); haiku[k] = v
    only = None
    if "--only" in args:
        only = args[args.index("--only") + 1].split(",")
    force = "--force" in args
    items = spec_items()
    report = {}
    for fn in funcs:
        if only and fn not in only:
            continue
        path = os.path.join(FNS, fn + ".rs")
        lines = open(path).read().split("\n")
        b, e = body_region(lines)
        if not force and not any("body not written yet" in l for l in lines[b:e]):
            report[fn] = "kept"
            continue
        if fn in haiku:
            hl = subprocess.run(["git", "show", f"{haiku[fn]}:logic/src/fns/{fn}.rs"], cwd=ROOT,
                                capture_output=True, text=True, check=True).stdout.split("\n")
            hb, he = body_region(hl)
            new = hl[hb:he]
            src = "haiku:" + haiku[fn]
        else:
            new = script_body(fn, funcs, items)
            src = "script"
        lines[b:e] = new
        open(path, "w").write("\n".join(lines))
        report[fn] = src
    if "--report" in args:
        json.dump(report, open(args[args.index("--report") + 1], "w"), indent=1)
    for k, v in report.items():
        print(f"{v:40s} {k}")


if __name__ == "__main__":
    main()
