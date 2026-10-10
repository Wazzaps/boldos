use std::env::var_os;
use std::process::Command;
use walkdir::WalkDir;

fn main() {
    let dirs = ["../init", "../example_bin", "../kernel_api"];
    for dir in dirs {
        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_entry(|e| e.file_name() != "target")
            .map(|e| e.expect(&format!("Failed to read {} directory", dir)))
            .filter(|e| e.file_type().is_file())
        {
            println!(
                "cargo:rerun-if-changed={}",
                entry.path().to_str().expect("File path was not UTF-8")
            );
        }
    }

    let out_dir = var_os("OUT_DIR").unwrap();
    Command::new("make")
        .env("OUT_DIR", out_dir)
        .current_dir("../init")
        .status()
        .unwrap()
        .success()
        .then_some(())
        .expect("Failed to compile init binary");
    Command::new("make")
        .current_dir("../example_bin")
        .status()
        .unwrap()
        .success()
        .then_some(())
        .expect("Failed to compile example-bin binary");
}
