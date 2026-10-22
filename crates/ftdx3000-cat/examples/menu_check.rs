//! Check raw `EXnnn...;` answers (one per line, from a file) against the menu
//! table: prints items whose width differs from the table or that don't decode.
//!
//!     cargo run -p ftdx3000-cat --example menu_check -- menus.txt

fn main() {
    let path = std::env::args().nth(1).expect("file with EX answers");
    let text = std::fs::read_to_string(path).expect("read file");
    let (mut ok, mut bad) = (0, 0);
    for line in text.lines() {
        let line = line.trim().trim_end_matches(';');
        let Some(rest) = line.strip_prefix("EX") else { continue };
        let Ok(num) = rest[..3].parse::<u16>() else { continue };
        let raw = &rest[3..];
        let item = ftdx3000_cat::menu::item(num).expect("menu item");
        let decoded = item.decode(raw);
        let width_ok = raw.len() == item.width as usize || matches!(item.kind, ftdx3000_cat::menu::Kind::Text { .. });
        if decoded.is_some() && width_ok {
            ok += 1;
        } else {
            bad += 1;
            println!(
                "{num:03} {:<26} raw={raw:<10} table width={} decoded={:?}  shown as: {}",
                item.name,
                item.width,
                decoded,
                item.display(raw)
            );
        }
    }
    println!("{ok} match, {bad} differ");
}
