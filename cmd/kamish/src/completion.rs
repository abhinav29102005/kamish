use clap::Command;
use clap_complete::{Shell, generate};
use std::io;

pub fn generate_completion(shell: Shell, cmd: &mut Command) {
    generate(shell, cmd, "kamish", &mut io::stdout());
}
