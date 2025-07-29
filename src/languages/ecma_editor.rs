use crate::{indentation::Indentation, languages::LanguageEditor};
use anyhow::{Result, anyhow};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

pub(super) struct EcmaEditor;
impl LanguageEditor for EcmaEditor {
    fn format_code(&self, source: &str, file_path: &Path) -> Result<String> {
        let mut command = Command::new("biome");
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg("format")
            .arg(format!("--stdin-file-path={}", file_path.display()))
            .arg("--diagnostic-level=error");

        match Indentation::determine(source).unwrap_or(Indentation::Spaces(2)) {
            Indentation::Spaces(spaces) => command
                .arg("--indent-style=space")
                .arg(format!("--indent-width={spaces}")),

            Indentation::Tabs => command.arg("--indent-style=tab"),
        };

        let mut child = command.spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(source.as_bytes())?;
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
}
