use std::process::Command;

fn main() {
    let status = Command::new("sh")
        .current_dir("initcode")
        .args(&["-c", "make clean && make initcode"])
        .status()
        .expect("Failed to execute make command");

    if !status.success() {
        panic!("make command failed");
    }
}