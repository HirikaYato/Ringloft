//! Отпечаток файла для сверки с `fpcalc`:
//! `cargo run --release --example fingerprint -- файл.flac`

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("использование: cargo run --example fingerprint -- <файл>");
        return;
    };
    match ringloft_lib::audio::fingerprint::fingerprint(std::path::Path::new(&path)) {
        Ok((duration, fingerprint)) => println!("DURATION={duration}\nFINGERPRINT={fingerprint}"),
        Err(err) => eprintln!("не вышло: {err}"),
    }
}
