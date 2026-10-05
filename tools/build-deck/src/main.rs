//! `build-deck`: the binary. See [`build_deck::run`].

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(build_deck::run(&args, &mut std::io::stderr()));
}
