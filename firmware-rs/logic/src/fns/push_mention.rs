// @fn push_mention
/// Append `@name ` unless the name is empty.
/// Contract: the result equals `spec_push_mention(t, who)`. No panic for any input.
pub fn push_mention(t: Text, who: &Name) -> Text {
    // @body
    if who.len == 0 || who.len as usize > NAME_CAP {
        return t;
    }
    let r = text_push(t, b"@");
    let r = text_push(r, &who.bytes[..who.len as usize]);
    text_push(r, b" ")
}
// @end

// @harness contract_push_mention target=push_mention stubs=text_push uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(text_push, opaque::o_text_push::f)]
#[kani::stub(spec_text_push, opaque::o_text_push::f)]
fn contract_push_mention() {
    let t: Text = kani::any();
    let who_val: Name = kani::any();
    let who: &Name = &who_val;
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_push_mention(t, who);
    opaque::end();
    let got = push_mention(t, who);
    assert!(opaque::gb_text_eq(&got, &want)); // the same as got == want (lemma_text_eq), cheaper to solve
}
// @end
