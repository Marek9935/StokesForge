//! `--version` and `-V` (#382).
use std::process::Command;

#[test]
fn both_flags_print_the_version_and_succeed() {
    for flag in ["--version", "-V"] {
        let output = Command::new(env!("CARGO_BIN_EXE_stokesforge-cli"))
            .arg(flag)
            .output()
            .unwrap();
        assert!(output.status.success(), "{flag}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("stokesforge-cli {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}"
        );
    }
}
