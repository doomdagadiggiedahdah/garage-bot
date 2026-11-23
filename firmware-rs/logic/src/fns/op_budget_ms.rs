// @fn op_budget_ms
/// Deadline the shell must enforce on an op.
/// Contract: the result equals `spec_op_budget_ms(op)`. No panic for any input.
pub fn op_budget_ms(op: Op) -> u64 {
    // @body
    match op {
        Op::Reboot => 0,
        Op::WifiConnect => 20_000,
        Op::Send(_) => 30_000,
        Op::Poll { .. } => 30_000,
        Op::NextUpdate => 1_000,
        Op::Ota => crate::MAX_OP_BUDGET_MS,
        Op::Idle => 1_000,
    }
}
// @end

// @harness contract_op_budget_ms target=op_budget_ms stubs= uses=
#[cfg(kani)]
#[kani::proof]
fn contract_op_budget_ms() {
    let op: Op = kani::any();
    let want = spec_op_budget_ms(op);
    let got = op_budget_ms(op);
    assert!(got == want);
}
// @end
