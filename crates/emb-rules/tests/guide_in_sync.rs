//! The engineer's guide must list every rule exactly as the code defines it.
//! After changing a rule, regenerate its table:
//! `cargo run -p emb-rules --example rule_table` and paste it into docs/GUIDE.md.

use emb_rules::engine::all_rules;
use emb_rules::model::Category;

const GUIDE: &str = include_str!("../../../docs/GUIDE.md");

#[test]
fn guide_lists_every_rule_as_built() {
    let mut missing = Vec::new();
    for r in all_rules() {
        let class = match (r.category, r.strict) {
            (Category::Mandatory, _) => "Mandatory",
            (Category::Required, true) => "Required, strict",
            (Category::Required, false) => "Required",
        };
        let row = format!("| {} | {} | {} | {} |", r.id, class, r.title, r.refs);
        if !GUIDE.contains(&row) {
            missing.push(row);
        }
    }
    assert!(missing.is_empty(), "docs/GUIDE.md is out of date; these rows are missing or changed:\n{}", missing.join("\n"));
}

#[test]
fn rule_ids_are_unique_and_cite_a_source() {
    let rules = all_rules();
    for (i, r) in rules.iter().enumerate() {
        assert!(rules[..i].iter().all(|o| o.id != r.id), "duplicate rule id {}", r.id);
        assert!(!r.refs.trim().is_empty() && !r.title.trim().is_empty(), "{} needs a title and a clause", r.id);
    }
}
