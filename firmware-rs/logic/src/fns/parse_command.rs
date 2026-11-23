// @fn parse_command
/// Recognise a command word the way main.py does.
/// Contract: the result equals `spec_parse_command(t)`. No panic for any input.
pub fn parse_command(t: &[u8]) -> Cmd {
    // @body
    let mut a = 0;
    while a < t.len() && ((t[a] >= 0x09 && t[a] <= 0x0d) || (t[a] >= 0x1c && t[a] <= 0x20)) {
        a += 1;
    }
    let mut e = t.len();
    while e > a && ((t[e - 1] >= 0x09 && t[e - 1] <= 0x0d) || (t[e - 1] >= 0x1c && t[e - 1] <= 0x20)) {
        e -= 1;
    }
    let mut k = a;
    while k < e && t[k] != b'@' {
        k += 1;
    }
    let w = &t[a..k];
    let is = |x: &[u8]| -> bool {
        if w.len() != x.len() {
            return false;
        }
        let mut i = 0;
        while i < x.len() {
            let c = if w[i] >= b'A' && w[i] <= b'Z' { w[i] + 32 } else { w[i] };
            if c != x[i] {
                return false;
            }
            i += 1;
        }
        true
    };
    if is(b"status") || is(b"/status") {
        Cmd::Status
    } else if is(b"open") || is(b"/open") {
        Cmd::Open
    } else if is(b"close") || is(b"/close") {
        Cmd::Close
    } else if is(b"silence") || is(b"/silence") || is(b"quiet") || is(b"stop") || is(b"mute") {
        Cmd::Silence
    } else if is(b"update") || is(b"/update") {
        Cmd::Update
    } else if is(b"debug") || is(b"/debug") || is(b"info") || is(b"/info") {
        Cmd::Diag
    } else if is(b"version") || is(b"/version") {
        Cmd::Version
    } else {
        Cmd::Unknown
    }
}
// @end

// @harness contract_parse_command target=parse_command stubs= uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(12)]
fn contract_parse_command() {
    let t_buf: [u8; 10] = kani::any();
    let t_len: usize = kani::any();
    kani::assume(t_len <= 10);
    let t: &[u8] = &t_buf[..t_len];
    let want = spec_parse_command(t);
    let got = parse_command(t);
    assert!(got == want);
}
// @end
