use std::process::{Command, Output};

fn main() {
    let Output { stdout, .. } = Command::new("git")
        .args(["show", "-s", "--pretty=%H %cI"])
        .output().unwrap();
    let value = String::from_utf8(stdout).unwrap();
    let (hash, date) = value.split_once(' ').unwrap();

    println!("cargo::rustc-env=SERVER_VERSION_HASH={}", &hash[..10]);
    println!("cargo::rustc-env=SERVER_VERSION_DATE={date}");
}
