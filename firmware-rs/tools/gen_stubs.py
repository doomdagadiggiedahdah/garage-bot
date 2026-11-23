#!/usr/bin/env python3
"""Generator for logic/src/fns/NAME.rs stubs, fns_list.rs and FUNCTIONS.json.
Run by the builder. It refuses to overwrite a file whose body is no longer the stub.
`gen_stubs.py --standins` prints the stand-in table that belongs in spec.rs (mod opaque)."""
import json, os, sys
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FNS = os.path.join(ROOT, "logic", "src", "fns")

T, U = ("t","Text",0), ("v","u64",0)
W = ("w","Walk",0)
# name, params [(name, type, slice bound)], return type, callees, spec name, one-line doc, extra
F = [
 # ---- time and the core
 ("uptime_s", [("now_ms","u64",0)], "u64", [], None, "Seconds since boot from the monotonic clock. Never accumulated, so it cannot drift."),
 ("interval_due", [("last","Option<u64>",0),("now","u64",0),("interval","u64",0)], "bool", [], "spec_due", "True when `interval` ms have passed since `last` (None = never ran = due). Saturating: a `last` in the future is not due unless interval is 0."),
 ("whole_minutes", [("elapsed_ms","u64",0)], "u64", [], None, "Whole minutes in a duration."),
 ("tenths_of_minute", [("elapsed_ms","u64",0)], "u64", [], "spec_tenths", "Minutes in tenths, rounded half up, saturating."),
 ("make_anchor", [("date","u64",0),("t_req_ms","u64",0),("now","u64",0)], "Option<Anchor>", [], "spec_anchor", "Build a time anchor from an HTTP Date header value, or None when the inputs are implausible."),
 ("boot_bound", [("k","Anchor",0)], "u64", [], "spec_boot_unix", "Upper bound on the unix second of this boot, from an anchor."),
 ("judge_age", [("anchor","Option<Anchor>",0),("boot_unix","Option<u64>",0),("now","u64",0),("date","Option<u64>",0)], "Age", [], "spec_age", "How old is a message dated `date`? Errs on the side of older."),
 ("decide", [("cmd","Cmd",0),("age","Age",0),("door_open","bool",0)], "Decision", [], None, "What a command does, given its age verdict and the door."),
 ("on_update", [("s","State",0),("u","UpdateIn",0),("now","u64",0),("door_open","bool",0)], "(State, bool)", ["judge_age","decide"], None, "Process one update of the batch. bool = press the relay now."),
 ("on_poll", [("s","State",0),("p","PollResult",0),("now","u64",0)], "State", ["make_anchor","boot_bound"], None, "Account for a finished getUpdates call."),
 ("closed_notice", [("d","DoorState",0),("now","u64",0)], "Option<ClosedNotice>", ["tenths_of_minute"], None, "The 'door closed after X.Y minutes' notice, when a bot close was pending and the door was open long enough."),
 ("open_alert_due", [("d","DoorState",0),("now","u64",0)], "bool", [], None, "Is an open-door alert due on this pass?"),
 ("close_timeout_due", [("c","CloseReq",0),("now","u64",0)], "bool", [], None, "Has a pending bot close timed out without being reported yet?"),
 ("door_tick", [("d","DoorState",0),("now","u64",0),("open","bool",0)], "(DoorState, DoorEvents)", ["closed_notice","open_alert_due","close_timeout_due","whole_minutes"], "spec_door", "Door monitoring for one pass: open alerts, close timeout, closed notice."),
 ("op_budget_ms", [("op","Op",0)], "u64", [], None, "Deadline the shell must enforce on an op."),
 ("deaf", [("last_good_poll","Option<u64>",0),("now","u64",0)], "bool", [], None, "No successful poll for DEAF_REBOOT_AFTER_MS (counted from boot when there was none yet)."),
 ("pick_op", [("n","State",0),("now","u64",0),("overrun","bool",0),("wifi_up","bool",0)], "Op", ["deaf","interval_due"], None, "The priority chain: which single op this pass performs."),
 ("overrun", [("last_step","Option<u64>",0),("now","u64",0)], "bool", [], None, "Was the previous pass more than the watchdog timeout ago, or did the clock go back?"),
 ("send_done", [("s","State",0),("m","Msg",0),("ok","bool",0)], "State", [], None, "The shell reports how the send of `m` went: after a failure `m` goes back into its slot, unless SEND_TRIES sends in a row have failed."),
 ("queue_door_events", [("s","State",0),("ev","DoorEvents",0),("door_open","bool",0)], "State", [], None, "Put this pass's door events into the pending slots; a closed door first drops the open-door ones."),
 ("wifi_track", [("since","Option<u64>",0),("now","u64",0),("wifi_up","bool",0)], "Option<u64>", [], None, "Since when WiFi has been down (None while it is up)."),
 ("op_done", [("s","State",0),("op","Op",0),("now","u64",0)], "State", [], None, "Clear the slot the chosen op serves, or stamp the poll time, or clear the update flag."),
 ("step", [("s","State",0),("i","StepIn",0)], "StepOut", ["overrun","door_tick","queue_door_events","wifi_track","interval_due","pick_op","op_done","uptime_s"], None, "One loop pass of the core."),
 # ---- text
 ("text_push", [T,("s","&[u8]",24)], "Text", [], None, "Append bytes; all or nothing; overflow is sticky."),
 ("div10", [U], "(u64, u8)", [], None, "One decimal step: `(v / 10, v % 10)`."),
 ("dec", [U], "([u8; 20], usize)", ["div10"], None, "Decimal digits of `v`, right-aligned in 20 bytes; digits are `b[i..]`, bytes before `i` are 0.",
  {"custom": True, "attrs":["#[kani::stub(div10, opaque::div10_opaque)]", "#[kani::stub(spec_div10, opaque::div10_opaque)]"],"pre":["opaque::div10_chain(v);"]}),
 ("div60", [U], "(u64, u64)", [], None, "Quotient and remainder by 60: `(v / 60, v % 60)`."),
 ("text_push_u64", [T,U], "Text", ["dec","text_push"], None, "Append a decimal number."),
 ("text_push_i64", [T,("v","i64",0)], "Text", ["text_push","text_push_u64"], None, "Append a signed decimal number."),
 ("text_push_pad6", [T,U], "Text", ["dec","text_push"], None, "Append a decimal number right-aligned with spaces in 6 columns (never truncated).",
  {"spec_call": "spec_text_push_padded(t, v, 6)", "spec_fn": "spec_text_push_padded", "spec_params": [T,U,("width","u8",0)]}),
 ("json_escape", [("c","u8",0)], "([u8; 6], u8)", [], None, "One byte escaped for the inside of a JSON string: the bytes and how many (1, 2 or 6)."),
 ("text_push_json", [T,("s","&[u8]",8)], "Text", ["json_escape","text_push"], None, "Append bytes escaped for the inside of a JSON string."),
 ("push_mention", [T,("who","&Name",0)], "Text", ["text_push"], None, "Append `@name ` unless the name is empty."),
 ("render_closed_after", [("c","ClosedNotice",0)], "Text", ["div10","push_mention","text_push","text_push_u64"], None, "`@name Door closed after X.Y minutes.`"),
 ("render_diag", [("version","&[u8]",8),("d","Diag",0)], "Text", ["div60","text_push","text_push_u64"], None, "The reply to the debug command."),
 ("render_reply", [("o","Outgoing",0),("version","&[u8]",8),("d","Diag",0)], "Text", ["text_push","push_mention","render_diag"], None, "The text of a reply to a command."),
 ("render_msg", [("m","Msg",0),("notify","&[u8]",8),("version","&[u8]",8),("d","Diag",0)], "Text", ["text_push","push_mention","text_push_u64","render_closed_after","render_reply"], None, "The Telegram message text for a Msg."),
 ("render_send_body", [("chat_id","i64",0),("text","&Text",0)], "Text", ["text_push","text_push_i64","text_push_json"], None, "JSON body for sendMessage."),
 ("render_poll_path", [("offset","u64",0)], "Text", ["text_push","text_push_u64"], None, "URL path and query for getUpdates."),
 ("render_log", [("uptime_s","u64",0),("level","Level",0),("free_mem","u32",0),("msg","&[u8]",8)], "Text", ["text_push","text_push_pad6"], None, "One log line in the format the log server parses."),
 ("render_heartbeat", [("d","Diag",0)], "Text", ["text_push","div60","text_push_u64","text_push_i64"], None, "The HEARTBEAT log message."),
 ("render_heap", [("d","Diag",0)], "Text", ["text_push","div60","text_push_u64"], None, "The HEAP telemetry line."),
 # ---- parsing
 ("parse_command", [("t","&[u8]",10)], "Cmd", [], None, "Recognise a command word the way main.py does."),
 ("parse_u64", [("s","&[u8]",21)], "Option<u64>", [], None, "1 to 19 ASCII digits."),
 ("parse_i64", [("s","&[u8]",21)], "Option<i64>", ["parse_u64"], None, "Optional minus sign, then 1 to 18 ASCII digits."),
 ("parse_http_date", [("s","&[u8]",30)], "Option<u64>", [], "spec_http_date", "IMF-fixdate HTTP Date header to unix seconds."),
 ("lex_step", [("lx","Lex",0),("b","u8",0)], "(Lex, Tok)", [], None, "JSON tokenizer, one byte."),
 ("classify_key", [("on","u8",0),("in_from","bool",0),("k","&[u8]",10)], "Key", [], None, "Which known JSON key the bytes spell at a path level."),
 ("name_from", [("s","&[u8]",34),("over","bool",0)], "Name", [], None, "Validate captured bytes as a username."),
 # ---- JSON structure tracker
 ("walk_tmp_append", [W,("b","u8",0)], "Walk", [], None, "Append one byte to the scratch buffer, or mark it overflowed when full."),
 ("walk_finish_scalar", [W], "Walk", ["parse_u64","parse_i64"], None, "A scalar value ended: store it where its key says and clear the scratch."),
 ("walk_open", [W,("is_obj","bool",0)], "Walk", [], None, "`{` or `[`: one level deeper, following the path to the fields of an update."),
 ("walk_close", [W], "(Walk, Option<RawUpdate>)", ["walk_finish_scalar"], None, "`}` or `]`: one level up; closing an update object hands the update out."),
 ("walk_str_start", [W], "Walk", [], None, "A string opens: decide what to capture (key, message text, username, nothing)."),
 ("walk_str_end", [W], "Walk", ["classify_key","name_from"], None, "A string closes: classify a key, or store the captured text or username."),
 ("walk_scalar", [W,("b","u8",0)], "Walk", ["walk_tmp_append"], None, "One byte of a number or literal: captured when its key is ok, update_id, date or id."),
 ("walk_step", [W,("t","Tok",0)], "(Walk, Option<RawUpdate>)", ["walk_open","walk_close","walk_finish_scalar","walk_str_start","walk_tmp_append","walk_str_end","walk_scalar"], None, "JSON structure tracker, one token."),
 ("scan_byte", [("sc","Scan",0),("b","u8",0)], "(Scan, Option<RawUpdate>)", ["lex_step","walk_step"], None, "Tokenizer plus tracker, one byte."),
 ("scan_finish", [("sc","Scan",0)], "bool", [], None, "Was the body one complete JSON value with ok:true?"),
 ("to_update_in", [("r","RawUpdate",0),("chat_id","i64",0)], "Option<UpdateIn>", ["parse_command"], None, "Reduce a parsed update to what the core needs."),
 ("inbox_push", [("q","Inbox",0),("u","RawUpdate",0)], "Inbox", [], None, "Queue a parsed update; a full inbox is unchanged."),
 ("inbox_pop", [("q","Inbox",0)], "(Inbox, Option<RawUpdate>)", [], None, "Take the oldest parsed update."),
 # ---- over-the-air update decisions
 ("version_newer", [("a","Version",0),("b","Version",0)], "bool", [], None, "`a` is a strictly later release than `b` (major, then minor, then patch)."),
 ("parse_version", [("s","&[u8]",26)], "Option<Version>", [], None, "`[v]MAJOR.MINOR.PATCH[+build]`, each part 1 to 5 digits and at most 65535."),
 ("ota_plan", [("running","Version",0),("m","Manifest",0),("slot_size","u32",0),("bad","Option<Version>",0)], "bool", ["version_newer"], None, "Download the manifest's image? Only a strictly newer version that fits the slot and is not the one that just rolled back."),
 ("ota_accept", [("running","Version",0),("m","Manifest",0),("slot_size","u32",0),("d","Download",0)], "bool", ["version_newer"], None, "Boot the downloaded image? Only if size, SHA-256 and embedded version match the manifest and the install is still allowed."),
 ("ota_mark_valid", [("pending_verify","bool",0),("last_good_poll","Option<u64>",0)], "bool", [], None, "Confirm a freshly installed image? Only after it completed a successful Telegram poll."),
 # ---- glue the shell calls
 ("bot_step", [("b","Bot",0),("i","StepIn",0)], "BotStep", ["interval_due","step"], None, "One loop pass. The shell calls this once per pass, right after feeding the watchdog."),
 ("bot_feed", [("b","Bot",0),("byte","u8",0)], "Bot", ["scan_byte","inbox_push"], None, "One byte of a getUpdates response body."),
 ("bot_poll_done", [("b","Bot",0),("http_ok","bool",0),("date_hdr","&[u8]",30),("t_req_ms","u64",0),("now","u64",0)], "Bot", ["scan_finish","parse_http_date","on_poll"], None, "The getUpdates exchange is over."),
 ("bot_next_update", [("b","Bot",0),("chat_id","i64",0),("now","u64",0),("door_open","bool",0)], "(Bot, bool)", ["inbox_pop","to_update_in","on_update"], None, "Act on the next queued update. bool = press the relay now."),
 ("bot_send_done", [("b","Bot",0),("m","Msg",0),("ok","bool",0)], "Bot", ["send_done"], None, "The shell calls this after every Op::Send, with the message and whether Telegram accepted it."),
 ("bot_http_result", [("b","Bot",0),("ok","bool",0),("sock_err","bool",0)], "Bot", [], None, "Count an HTTP call for the diagnostics."),
]
ROW = {r[0]: r for r in F}
def extra(n): return ROW[n][6] if len(ROW[n]) > 6 else {}
def spec_fn(n): return extra(n).get("spec_fn") or ROW[n][4] or "spec_" + n
def spec_params(n): return extra(n).get("spec_params") or ROW[n][1]

# Loop limit per `contract_f` harness, for the loops of the function itself. Kani
# fails the proof ("unwinding assertion") when a loop can run longer than the
# limit, so a limit that is too small can never make a wrong proof pass. A body
# author may change this one number in the harness of their own function;
# nothing else in a harness. (Byte comparisons and the stand-in key copy have
# their own fixed limits in logic/Cargo.toml.)
UNWIND = {
 "text_push": 26, "dec": 22, "text_push_pad6": 8, "text_push_json": 10,
 "parse_command": 12, "parse_u64": 23, "parse_i64": 23, "parse_http_date": 32,
 "classify_key": 12, "name_from": 36, "walk_str_end": 66, "inbox_push": 10, "inbox_pop": 10,
 "parse_version": 28, "ota_accept": 34,
}

# Stand-ins (spec.rs, mod opaque): slots = how many different calls one proof may remember.
SLOTS = {"spec_text_push": 12, "spec_text_push_u64": 8, "spec_json_escape": 8}
KEYBYTES = {"spec_text_push_json": 512}     # longest slice a key can hold (default 96)
# The one thing a proof may know about a stand-in's answer; each is proven by a lemma in props.rs.
FACT = {"spec_dec": "|v: &([u8; 20], usize)| v.1 < 20",        # lemma_dec_index
        "spec_json_escape": "|v: &([u8; 6], u8)| v.1 <= 6",    # lemma_json_escape_len
        "spec_parse_u64": "|v: &Option<u64>| s.len() > 18 || match v { Some(x) => *x < 1_000_000_000_000_000_000, None => true }"}  # lemma_parse_u64_18

def standin_of(c):
    """Path (inside mod opaque) of the stand-in for function c."""
    sp = spec_fn(c)
    return f"w_{c}" if extra(c).get("spec_call") else f"o_{sp[5:]}::f"

def standins():
    used = []
    for r in F:
        if extra(r[0]).get("custom"): continue
        for c in r[3]:
            if c not in used: used.append(c)
    out = []
    for c in sorted(used, key=lambda c: [r[0] for r in F].index(c)):
        sp = spec_fn(c); ps = spec_params(c)
        kb = KEYBYTES.get(sp, 96)
        comps = []
        for n, t, _ in ps:
            if t == "&[u8]": comps.append((0, f"SK<{kb // 16}>", f"sk({n})"))
            elif t == "&Text": comps.append((1, "Text", "*" + n))
            elif t == "Text": comps.append((1, "Text", n))
            elif t.startswith("&"): comps.append((0, t[1:], "*" + n))
            else: comps.append((0, t, n))
        comps.sort(key=lambda c: c[0])      # cheap comparisons first, Texts last
        kt = [c[1] for c in comps]; ke = [c[2] for c in comps]
        eq = " && ".join((f"gb_text_eq(&a.{i}, &b.{i})" if c[0] else f"a.{i} == b.{i}") for i, c in enumerate(comps))
        sig = ", ".join(f"{n}: {t}" for n, t, _ in ps)
        seq = "true" if any(c[0] for c in comps) else "false"
        out.append(f"    standin!(o_{sp[5:]}, {SLOTS.get(sp, 4)}, {seq}, ({sig}) -> {ROW[c][2]}, ({', '.join(kt)},), ({', '.join(ke)},), |a, b| {eq}, {FACT.get(sp, '|_| true')});")
    names = [o.split("(")[1].split(",")[0] for o in out]
    for c in used:
        if extra(c).get("spec_call"):   # a function whose specification is another one's with fixed arguments
            sig = ", ".join(f"{n}: {t}" for n, t, _ in ROW[c][1])
            call = extra(c)["spec_call"].replace(spec_fn(c) + "(", f"o_{spec_fn(c)[5:]}::f(", 1)
            out.append(f"    pub mod m_{c} {{\n        #[allow(unused_imports)]\n        use super::*;\n        #[allow(unused_imports)]\n        use crate::*;\n"
                       f"        pub fn w_{c}({sig}) -> {ROW[c][2]} {{\n            {call}\n        }}\n    }}\n    pub use m_{c}::w_{c};")
    out.append("    /// Empty every stand-in table and start recording.")
    out.append("    pub fn begin() {\n        unsafe {\n" + "".join(f"            {n}::N = 0;\n            {n}::J = 0;\n" for n in names) + "            REC = true;\n        }\n    }")
    return "\n".join(out) + "\n"

def gen(name, params, ret, callees, spec, doc, ex=None):
    ex = ex or {}
    spec = spec or "spec_" + name
    sig = ", ".join(f"{n}: {t}" for n, t, _ in params)
    args = ", ".join(n for n, _, _ in params)
    scall = ex.get("spec_call") or f"{spec}({args})"
    lines = [f"// @fn {name}", f"/// {doc}", f"/// Contract: the result equals `{scall}`. No panic for any input."]
    lines += [f"pub fn {name}({sig}) -> {ret} {{", "    // @body",
              f"    let _ = ({args});", f"    unimplemented!(\"{name}: body not written yet\")", "}", "// @end", ""]
    h = [f"// @harness contract_{name} target={name} stubs={','.join(callees)} uses=", "#[cfg(kani)]", "#[kani::proof]"]
    if name in UNWIND:
        h += [f"#[kani::unwind({UNWIND[name]})]"]
    h += ex.get("attrs", [])
    replay = bool(callees) and not ex.get("custom")
    if replay:
        seen = []
        for c in callees:
            sp = spec_fn(c)
            h += [f"#[kani::stub({c}, opaque::{standin_of(c)})]"]
            if sp not in seen:
                seen.append(sp); h += [f"#[kani::stub({sp}, opaque::o_{sp[5:]}::f)]"]
    h += [f"fn contract_{name}() {{"]
    call = []
    for n, t, bound in params:
        if t == "&[u8]":
            h += [f"    let {n}_buf: [u8; {bound}] = kani::any();", f"    let {n}_len: usize = kani::any();", f"    kani::assume({n}_len <= {bound});",
                  f"    let {n}: &[u8] = &{n}_buf[..{n}_len];"]
            call.append(n)
        elif t.startswith("&"):
            h += [f"    let {n}_val: {t[1:]} = kani::any();", f"    let {n}: {t} = &{n}_val;"]
            call.append(n)
        else:
            h += [f"    let {n}: {t} = kani::any();"]
            call.append(n)
    h += ["    " + x for x in ex.get("pre", [])]
    if replay:
        h += ["    // Every callee and its specification are replaced by the same stand-in. Evaluate",
              "    // the specification once so the stand-ins fix their answers, then check the function.",
              "    opaque::begin();", f"    let want = {scall};", "    opaque::end();"]
    else:
        h += [f"    let want = {scall};"]
    h += [f"    let got = {name}({', '.join(call)});", ("    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve" if ret == "Text" else "    assert!(got == want);"), "}", "// @end", ""]
    return "\n".join(lines + h)

if __name__ == "__main__":
    if "--specmap" in sys.argv:
        print(json.dumps({r[0]: spec_fn(r[0]) for r in F if not extra(r[0]).get("spec_call")})); sys.exit(0)
    if "--standins" in sys.argv:
        sys.stdout.write(standins()); sys.exit(0)
    os.makedirs(FNS, exist_ok=True)
    for row in F:
        path = os.path.join(FNS, row[0] + ".rs")
        if os.path.exists(path) and "body not written yet" not in open(path).read():
            print("keep (has a body):", row[0]); continue
        open(path, "w").write(gen(*row))
    for f in sorted(os.listdir(FNS)):
        if f.endswith(".rs") and f[:-3] not in ROW:
            print("not in the table (remove by hand):", f)
    open(os.path.join(ROOT, "logic", "src", "fns_list.rs"), "w").write("".join(f'include!("fns/{r[0]}.rs");\n' for r in F))
    json.dump([{"name": r[0], "sig": f"{r[0]}({', '.join(n+': '+t for n,t,_ in r[1])}) -> {r[2]}", "callees": r[3],
                "spec": extra(r[0]).get("spec_call") or (r[4] or "spec_"+r[0]),
                "doc": r[5], "bounds": {n: b for n,t,b in r[1] if b}} for r in F], open(os.path.join(ROOT, "FUNCTIONS.json"), "w"), indent=1)
    print(len(F), "functions,", sum(1 for r in F if not r[3]), "leaves")
