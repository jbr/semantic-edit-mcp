//! Running a formatter program: the source on its stdin, the result on its
//! stdout.

use anyhow::{Result, anyhow};
use std::{
    ffi::OsStr,
    io::{Read, Write},
    process::{Command, Stdio},
};

/// Run `program` with `args` on `source`. Its stdout is the result when it
/// succeeds, and its stderr is the error when it doesn't.
pub(super) fn run(
    program: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    source: &str,
) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(source.as_bytes())?;
        // Closing stdin ends the input.
        drop(stdin);
    }

    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_string(&mut stdout)?;
    }

    let mut stderr = String::new();
    if let Some(mut err) = child.stderr.take() {
        err.read_to_string(&mut stderr)?;
    }

    if child.wait()?.success() {
        Ok(stdout)
    } else {
        Err(anyhow!(stderr))
    }
}
