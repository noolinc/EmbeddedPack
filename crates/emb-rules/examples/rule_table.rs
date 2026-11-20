//! Prints every rule as Markdown tables, one per pack: `cargo run -p emb-rules --example rule_table`.
use emb_rules::engine::all_rules;
use emb_rules::model::Category;

fn main() {
    let mut pack = None;
    for r in all_rules() {
        if pack != Some(r.pack) {
            pack = Some(r.pack);
            println!("\n### {:?}\n\n| Rule | Class | Requirement | Clauses |\n|---|---|---|---|", r.pack);
        }
        let class = match (r.category, r.strict) {
            (Category::Mandatory, _) => "Mandatory",
            (Category::Required, true) => "Required, strict",
            (Category::Required, false) => "Required",
        };
        println!("| {} | {} | {} | {} |", r.id, class, r.title, r.refs);
    }
}
