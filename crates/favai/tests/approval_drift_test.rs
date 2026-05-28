// Regression guard: after a sync that modifies a bundle on disk,
// the registry must quarantine it until the operator approves the new hash.
//
// This test is a placeholder — it requires SkillRegistry integration
// (the starter-skills crate) and is wired up in Step 2 of the ship plan.
#[test]
#[ignore = "requires starter-skills integration (Step 2)"]
fn modified_bundle_is_quarantined_after_reload() {
    todo!()
}
