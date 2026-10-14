// cargo run --example say -- <chitter|grumble|blip|babble> <text>
use wurble::Voice;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let voice = match args.next().as_deref() {
        Some("grumble") => Voice::grumble(),
        Some("blip") => Voice::blip(),
        Some("babble") => Voice::babble(),
        _ => Voice::chitter(),
    };
    std::fs::write("wurble.wav", voice.wav(&args.collect::<Vec<_>>().join(" ")))
}
