//! `learn-cli`: the binary.

use std::env;
use std::io::{stderr, stdin, stdout};

fn main() {
    let code = learn_cli::run(env::args().skip(1), stdin().lock(), stdout(), stderr());
    std::process::exit(code);
}
