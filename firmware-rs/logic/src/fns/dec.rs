// @fn dec
/// Decimal digits of `v`, right-aligned in 20 bytes; digits are `b[i..]`, bytes before `i` are 0.
/// Contract: the result equals `spec_dec(v)`. No panic for any input.
pub fn dec(v: u64) -> ([u8; 20], usize) {
    // @body
    let mut b = [0u8; 20];
    let mut i: usize = 20;
    let mut x = v;
    loop {
        i -= 1;
        let (q, r) = div10(x);
        b[i] = b'0' + r;
        x = q;
        if x == 0 {
            break;
        }
    }
    (b, i)
}
// @end

// @harness contract_dec target=dec stubs=div10 uses=
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(22)]
#[kani::stub(div10, opaque::div10_opaque)]
#[kani::stub(spec_div10, opaque::div10_opaque)]
fn contract_dec() {
    let v: u64 = kani::any();
    opaque::div10_chain(v);
    let want = spec_dec(v);
    let got = dec(v);
    assert!(got == want);
}
// @end
