// @fn json_escape
/// One byte escaped for the inside of a JSON string: the bytes and how many (1, 2 or 6).
/// Contract: the result equals `spec_json_escape(c)`. No panic for any input.
pub fn json_escape(c: u8) -> ([u8; 6], u8) {
    // @body
    fn h_hex(n: u8) -> u8 {
        if n < 10 {
            b'0' + n
        } else {
            b'a' + (n - 10)
        }
    }
    if c == b'"' {
        ([b'\\', b'"', 0, 0, 0, 0], 2)
    } else if c == b'\\' {
        ([b'\\', b'\\', 0, 0, 0, 0], 2)
    } else if c == b'\n' {
        ([b'\\', b'n', 0, 0, 0, 0], 2)
    } else if c < 0x20 {
        ([b'\\', b'u', b'0', b'0', h_hex(c >> 4), h_hex(c & 15)], 6)
    } else {
        ([c, 0, 0, 0, 0, 0], 1)
    }
}
// @end

// @harness contract_json_escape target=json_escape stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_json_escape() {
    let c: u8 = kani::any();
    let want = spec_json_escape(c);
    let got = json_escape(c);
    assert!(got == want);
}
// @end
