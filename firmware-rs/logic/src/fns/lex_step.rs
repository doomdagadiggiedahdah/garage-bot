// @fn lex_step
/// JSON tokenizer, one byte.
/// Contract: the result equals `spec_lex_step(lx, b)`. No panic for any input.
pub fn lex_step(lx: Lex, b: u8) -> (Lex, Tok) {
    // @body
    let to = |mode: LexMode, t: Tok| (Lex { mode, u_left: 0, u_acc: 0 }, t);
    match lx.mode {
        LexMode::Err => to(LexMode::Err, Tok::Bad),
        LexMode::Value => match b {
            b'{' => to(LexMode::Value, Tok::ObjOpen),
            b'}' => to(LexMode::Value, Tok::ObjClose),
            b'[' => to(LexMode::Value, Tok::ArrOpen),
            b']' => to(LexMode::Value, Tok::ArrClose),
            b':' => to(LexMode::Value, Tok::Colon),
            b',' => to(LexMode::Value, Tok::Comma),
            b' ' | b'\t' | b'\n' | b'\r' => to(LexMode::Value, Tok::Skip),
            b'"' => to(LexMode::Str, Tok::StrStart),
            b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'+' | b'-' | b'.' => to(LexMode::Value, Tok::Scalar(b)),
            _ => to(LexMode::Err, Tok::Bad),
        },
        LexMode::Str => match b {
            b'"' => to(LexMode::Value, Tok::StrEnd),
            b'\\' => to(LexMode::Esc, Tok::Skip),
            0..=0x1f => to(LexMode::Err, Tok::Bad),
            _ => to(LexMode::Str, Tok::StrByte(b)),
        },
        LexMode::Esc => match b {
            b'"' | b'\\' | b'/' => to(LexMode::Str, Tok::StrByte(b)),
            b'b' => to(LexMode::Str, Tok::StrByte(0x08)),
            b'f' => to(LexMode::Str, Tok::StrByte(0x0c)),
            b'n' => to(LexMode::Str, Tok::StrByte(0x0a)),
            b'r' => to(LexMode::Str, Tok::StrByte(0x0d)),
            b't' => to(LexMode::Str, Tok::StrByte(0x09)),
            b'u' => (Lex { mode: LexMode::Uni, u_left: 4, u_acc: 0 }, Tok::Skip),
            _ => to(LexMode::Err, Tok::Bad),
        },
        LexMode::Uni => {
            let v: u8 = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return to(LexMode::Err, Tok::Bad),
            };
            if lx.u_left == 0 || lx.u_left > 4 {
                return to(LexMode::Err, Tok::Bad);
            }
            let acc = (lx.u_acc & 0x0fff) * 16 + u16::from(v);
            if lx.u_left == 1 {
                // code points above 0x7f (and surrogates) become '?'
                let out = if acc < 0x80 { acc as u8 } else { b'?' };
                to(LexMode::Str, Tok::StrByte(out))
            } else {
                (Lex { mode: LexMode::Uni, u_left: lx.u_left - 1, u_acc: acc }, Tok::Skip)
            }
        }
    }
}
// @end

// @harness contract_lex_step target=lex_step stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_lex_step() {
    let lx: Lex = kani::any();
    let b: u8 = kani::any();
    let want = spec_lex_step(lx, b);
    let got = lex_step(lx, b);
    assert!(got == want);
}
// @end
