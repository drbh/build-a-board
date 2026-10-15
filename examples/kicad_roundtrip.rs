//! Parse and reprint KiCad s-expression files; report any that change.
fn main() {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let again = build_a_board::sexpr::parse(&text).unwrap().to_kicad();
        if again == text {
            println!("identical  {path}");
        } else {
            let at = text.bytes().zip(again.bytes()).position(|(a, b)| a != b).unwrap_or(text.len().min(again.len()));
            println!(
                "DIFFERS    {path} at byte {at}: {:?} vs {:?}",
                &text[at.saturating_sub(40)..(at + 40).min(text.len())],
                &again[at.saturating_sub(40)..(at + 40).min(again.len())]
            );
        }
    }
}
