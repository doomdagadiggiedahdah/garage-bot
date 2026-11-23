// @fn to_update_in
/// Reduce a parsed update to what the core needs.
/// Contract: the result equals `spec_to_update_in(r, chat_id)`. No panic for any input.
pub fn to_update_in(r: RawUpdate, chat_id: i64) -> Option<UpdateIn> {
    // @body
    let id = r.update_id?;
    let cmd = if r.has_text && !r.text_over && r.text_len as usize <= CMD_TEXT_CAP {
        parse_command(&r.text[..r.text_len as usize])
    } else {
        Cmd::Unknown
    };
    Some(UpdateIn {
        update_id: id,
        authorized: r.has_text && r.chat_id == Some(chat_id),
        cmd,
        date: r.date,
        who: r.who,
    })
}
// @end

// @harness contract_to_update_in target=to_update_in stubs=parse_command uses=
#[cfg(kani)]
#[kani::proof]
#[kani::stub(parse_command, opaque::o_parse_command::f)]
#[kani::stub(spec_parse_command, opaque::o_parse_command::f)]
fn contract_to_update_in() {
    let r: RawUpdate = kani::any();
    let chat_id: i64 = kani::any();
    // Every callee and its specification are replaced by the same stand-in. Evaluate
    // the specification once so the stand-ins fix their answers, then check the function.
    opaque::begin();
    let want = spec_to_update_in(r, chat_id);
    opaque::end();
    let got = to_update_in(r, chat_id);
    assert!(got == want);
}
// @end
