//! Running a formatter program: the source on its stdin, the result on its
//! stdout.
//!
//! stdin is written while stdout and stderr are read, each on a thread of its
//! own, so a program that writes before it has read all of its input, or that
//! fills its stderr, can't deadlock against this process. The whole run has a
//! deadline, past which the program is killed.

use super::{Failure, resolve::resolve};
use std::{
    ffi::OsString,
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

/// How long a formatter has to finish before it is killed and the edit is
/// written without it. Formatters take well under a second on files of any
/// ordinary size; this leaves room for a slow first run and a loaded machine.
pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

/// The longest wait between checks on whether the program has exited.
const MAX_POLL: Duration = Duration::from_millis(20);

/// Run `program`, found as [`resolve`] finds it, with `args`, in `dir` when it
/// is a directory, on `source`, for at most `timeout`. Its stdout is the result
/// when it succeeds.
///
/// It is [`Failure::Unavailable`] when it can't be started or doesn't finish
/// in time, and [`Failure::Failed`] when it exits unsuccessfully, with its
/// stderr as the error. Succeeding without reading all of its input, or with
/// output that is empty for a source that isn't, or that isn't UTF-8, is a
/// failure too: none of those can be the source formatted.
pub(super) fn run(
    program: &str,
    args: &[OsString],
    dir: Option<&Path>,
    source: &str,
    timeout: Duration,
) -> Result<String, Failure> {
    let deadline = Instant::now() + timeout;
    let path = resolve(program).map_err(Failure::Unavailable)?;
    let mut command = Command::new(&path);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A directory that doesn't exist would fail the spawn, and read as a
    // program that can't run.
    if let Some(dir) = dir.filter(|dir| dir.is_dir()) {
        command.current_dir(dir);
    }
    let mut child = command
        .spawn()
        .map_err(|error| Failure::Unavailable(error.to_string()))?;

    let streams = match Streams::start(&mut child, source) {
        Ok(streams) => streams,
        Err(error) => {
            kill(&mut child);
            return Err(Failure::Unavailable(error.to_string()));
        }
    };
    // The pipes close when the program exits, so waiting on them first finds
    // it exited without polling for it, unless something it started still
    // holds them.
    let finished = streams.finish(deadline);
    let status = match wait(&mut child, deadline) {
        Ok(Some(status)) => status,
        Ok(None) => {
            kill(&mut child);
            return Err(timed_out(timeout));
        }
        Err(error) => {
            kill(&mut child);
            return Err(Failure::Unavailable(error.to_string()));
        }
    };
    let Some(Finished {
        input,
        stdout,
        stderr,
    }) = finished
    else {
        return Err(timed_out(timeout));
    };

    if !status.success() {
        return Err(Failure::Failed(error_text(&stderr, &stdout, status)));
    }
    if input.is_err() {
        return Err(Failure::Failed(String::from(
            "it exited without reading all of its input",
        )));
    }
    let output = String::from_utf8(stdout.unwrap_or_default())
        .map_err(|_| Failure::Failed(String::from("its output isn't UTF-8")))?;
    if output.trim().is_empty() && !source.trim().is_empty() {
        return Err(Failure::Failed(String::from("it wrote nothing to stdout")));
    }
    Ok(output)
}

/// The program's exit status, or `None` if it is still running at `deadline`.
fn wait(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    let mut poll = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        thread::sleep(poll.min(deadline - now));
        poll = (poll * 2).min(MAX_POLL);
    }
}

/// End the program and reap it. Whatever it started that still holds its
/// pipes is left to finish on its own: the threads reading them end when it
/// does, and nothing waits for them.
fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn timed_out(timeout: Duration) -> Failure {
    Failure::Unavailable(format!("timed out after {}", seconds(timeout)))
}

/// `duration` in seconds, as briefly as it reads correctly.
fn seconds(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds.fract() == 0.0 {
        format!("{seconds}s")
    } else {
        format!("{seconds:.1}s")
    }
}

/// What a failed program said: its stderr, or its stdout if it wrote nothing to
/// stderr, or its exit status if it wrote nothing at all.
fn error_text(
    stderr: &io::Result<Vec<u8>>,
    stdout: &io::Result<Vec<u8>>,
    status: ExitStatus,
) -> String {
    [stderr, stdout]
        .into_iter()
        .filter_map(|stream| stream.as_ref().ok())
        .map(|bytes| String::from_utf8_lossy(bytes).trim_end().to_owned())
        .find(|text| !text.trim().is_empty())
        .unwrap_or_else(|| format!("it exited with {status}"))
}

/// The threads writing the program's stdin and reading its stdout and stderr.
struct Streams {
    input: Receiver<io::Result<()>>,
    stdout: Receiver<io::Result<Vec<u8>>>,
    stderr: Receiver<io::Result<Vec<u8>>>,
}

/// What the threads in [`Streams`] came back with.
struct Finished {
    input: io::Result<()>,
    stdout: io::Result<Vec<u8>>,
    stderr: io::Result<Vec<u8>>,
}

impl Streams {
    fn start(child: &mut Child, source: &str) -> io::Result<Self> {
        let missing = || io::Error::other("the program's pipes weren't set up");
        let mut stdin = child.stdin.take().ok_or_else(missing)?;
        let stdout = child.stdout.take().ok_or_else(missing)?;
        let stderr = child.stderr.take().ok_or_else(missing)?;
        let source = source.as_bytes().to_vec();
        Ok(Self {
            // Dropping stdin when the write is done closes it, which ends the input.
            input: spawn("formatter stdin", move || stdin.write_all(&source))?,
            stdout: spawn("formatter stdout", move || read(stdout))?,
            stderr: spawn("formatter stderr", move || read(stderr))?,
        })
    }

    /// Each thread's result, or `None` if any is still running at `deadline`.
    fn finish(self, deadline: Instant) -> Option<Finished> {
        fn by<T>(receiver: &Receiver<T>, deadline: Instant) -> Option<T> {
            let remaining = deadline.saturating_duration_since(Instant::now());
            // Disconnected only if the thread panicked, which reads the same.
            receiver.recv_timeout(remaining).ok()
        }
        Some(Finished {
            stdout: by(&self.stdout, deadline)?,
            stderr: by(&self.stderr, deadline)?,
            input: by(&self.input, deadline)?,
        })
    }
}

fn read(mut stream: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Run `work` on a thread of its own, its result sent when it is done. The
/// thread isn't joined, so one blocked on a pipe that something else holds
/// open never holds up the edit.
fn spawn<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> io::Result<Receiver<T>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let _ = sender.send(work());
        })?;
    Ok(receiver)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    const SHORT: Duration = Duration::from_millis(500);

    fn sh(script: &str, source: &str, timeout: Duration) -> Result<String, Failure> {
        run(
            "sh",
            &[OsString::from("-c"), OsString::from(script)],
            None,
            source,
            timeout,
        )
    }

    /// More than a pipe holds, so a runner that writes all of stdin before
    /// reading stdout deadlocks against a program that writes as it reads.
    #[test]
    fn input_larger_than_a_pipe_round_trips() {
        let source = "let x = 1;\n".repeat(200_000);
        assert_eq!(sh("cat", &source, TIMEOUT).unwrap(), source);
    }

    /// A runner that reads stderr only once stdout closes deadlocks against a
    /// program that fills stderr before it writes stdout.
    #[test]
    fn a_flood_of_stderr_doesnt_block_the_output() {
        let source = "let x = 1;\n".repeat(20_000);
        let script = "head -c 1000000 /dev/zero | tr '\\0' w >&2; cat";
        assert_eq!(sh(script, &source, TIMEOUT).unwrap(), source);
    }

    #[test]
    fn a_hung_program_is_killed_at_the_timeout() {
        let started = Instant::now();
        let result = sh("sleep 30", "x\n", SHORT);
        assert_eq!(
            result,
            Err(Failure::Unavailable(String::from("timed out after 0.5s")))
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    /// The shell runs `sleep` as a child of its own, which holds the pipes
    /// open after the shell is killed: the run still ends at the timeout.
    #[test]
    fn a_hung_programs_children_dont_hold_up_the_timeout() {
        let started = Instant::now();
        let result = sh("sleep 30; cat", "x\n", SHORT);
        assert!(matches!(result, Err(Failure::Unavailable(_))), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_failure_is_its_stderr() {
        assert_eq!(
            sh("echo 'cannot format this' >&2; exit 3", "x\n", TIMEOUT),
            Err(Failure::Failed(String::from("cannot format this")))
        );
    }

    #[test]
    fn a_failure_without_stderr_is_its_stdout_or_its_status() {
        assert_eq!(
            sh(
                "cat >/dev/null; echo 'said on stdout'; exit 1",
                "x\n",
                TIMEOUT
            ),
            Err(Failure::Failed(String::from("said on stdout")))
        );
        assert_eq!(
            sh("cat >/dev/null; exit 3", "x\n", TIMEOUT),
            Err(Failure::Failed(String::from(
                "it exited with exit status: 3"
            )))
        );
    }

    /// A program that succeeds without writing anything, like one that
    /// formats a file in place, would otherwise empty the file.
    #[test]
    fn no_output_for_a_source_is_a_failure() {
        assert_eq!(
            sh("cat >/dev/null", "fn main() {}\n", TIMEOUT),
            Err(Failure::Failed(String::from("it wrote nothing to stdout")))
        );
        assert_eq!(sh("cat", "\n", TIMEOUT).unwrap(), "\n");
    }

    /// A program that succeeds without reading its input can't have
    /// formatted it. More input than a pipe holds makes that observable.
    #[test]
    fn succeeding_without_reading_the_input_is_a_failure() {
        let source = "let x = 1;\n".repeat(200_000);
        assert_eq!(
            sh("echo 'fn main() {}'", &source, TIMEOUT),
            Err(Failure::Failed(String::from(
                "it exited without reading all of its input"
            )))
        );
    }

    #[test]
    fn output_that_isnt_utf8_is_a_failure() {
        assert_eq!(
            sh("cat >/dev/null; printf '\\377'", "x\n", TIMEOUT),
            Err(Failure::Failed(String::from("its output isn't UTF-8")))
        );
    }

    #[test]
    fn a_missing_program_is_unavailable() {
        assert_eq!(
            run("semantic-edit-no-such-formatter", &[], None, "x\n", TIMEOUT),
            Err(Failure::Unavailable(String::from("not found on PATH")))
        );
    }

    #[test]
    fn it_runs_in_the_directory_it_is_given() {
        let dir = std::env::temp_dir().join(format!("semantic-edit-runner-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let result = run(
            "sh",
            &[
                OsString::from("-c"),
                OsString::from("cat >/dev/null; pwd -P"),
            ],
            Some(&dir),
            "x\n",
            TIMEOUT,
        );
        let expected = fs::canonicalize(&dir).unwrap();
        fs::remove_dir_all(&dir).ok();
        assert_eq!(result.unwrap().trim_end(), expected.to_str().unwrap());
    }

    /// A directory that doesn't exist isn't a reason the program can't run.
    #[test]
    fn a_directory_that_doesnt_exist_is_ignored() {
        let result = run(
            "cat",
            &[],
            Some(Path::new("/semantic-edit-nonexistent/dir")),
            "x\n",
            TIMEOUT,
        );
        assert_eq!(result.unwrap(), "x\n");
    }

    #[test]
    fn durations_read_briefly() {
        assert_eq!(seconds(Duration::from_secs(10)), "10s");
        assert_eq!(seconds(Duration::from_millis(500)), "0.5s");
        assert_eq!(seconds(Duration::from_millis(1300)), "1.3s");
    }
}
