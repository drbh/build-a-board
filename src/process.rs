//! External tools: KiCad's CLI, FreeRouting, ImageMagick, and check scripts.

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use thiserror::Error;
use wait_timeout::ChildExt;

#[derive(Debug, Error)]
pub enum RunError {
    #[error("could not start {program}: {source}")]
    Spawn { program: String, source: std::io::Error },
    #[error("{command} failed ({status})")]
    Failed { command: String, status: ExitStatus },
    #[error("{command} timed out after {seconds} s")]
    TimedOut { command: String, seconds: u64 },
}

impl RunError {
    /// The tool ran but failed or stalled (as opposed to not starting).
    pub fn is_tool_failure(&self) -> bool {
        matches!(self, Self::Failed { .. } | Self::TimedOut { .. })
    }
}

/// A command line, echoed as `+ program args...` when run.
#[derive(Clone, Debug)]
pub struct Cmd {
    argv: Vec<OsString>,
    cwd: Option<PathBuf>,
    timeout: Option<Duration>,
}

impl Cmd {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self { argv: vec![program.as_ref().to_owned()], cwd: None, timeout: None }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.argv.push(arg.as_ref().to_owned());
        self
    }

    pub fn args<I>(mut self, args: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        self.argv.extend(args.into_iter().map(|a| a.as_ref().to_owned()));
        self
    }

    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn line(&self) -> String {
        self.argv.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ")
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.argv[0]);
        command.args(&self.argv[1..]);
        if let Some(dir) = &self.cwd {
            command.current_dir(dir);
        }
        command
    }

    fn spawn_error(&self, source: std::io::Error) -> RunError {
        RunError::Spawn { program: self.argv[0].to_string_lossy().into_owned(), source }
    }

    /// Echo and run with inherited output.
    pub fn run(&self) -> Result<(), RunError> {
        println!("+ {}", self.line());
        self.wait(self.command())
    }

    /// Run without echoing, discarding output unless it fails.
    pub fn run_quiet(&self) -> Result<(), RunError> {
        let output = self.command().stdin(Stdio::null()).output().map_err(|e| self.spawn_error(e))?;
        if output.status.success() {
            return Ok(());
        }
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        Err(RunError::Failed { command: self.line(), status: output.status })
    }

    fn wait(&self, mut command: Command) -> Result<(), RunError> {
        let mut child = command.spawn().map_err(|e| self.spawn_error(e))?;
        let status = match self.timeout {
            None => child.wait().map_err(|e| self.spawn_error(e))?,
            Some(timeout) => match child.wait_timeout(timeout).map_err(|e| self.spawn_error(e))? {
                Some(status) => status,
                None => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RunError::TimedOut { command: self.line(), seconds: timeout.as_secs() });
                }
            },
        };
        if status.success() { Ok(()) } else { Err(RunError::Failed { command: self.line(), status }) }
    }

    /// Run silently; report success.
    pub fn succeeds(&self) -> Result<bool, RunError> {
        self.command().status().map(|s| s.success()).map_err(|e| self.spawn_error(e))
    }

    /// Capture stdout while the tool's own messages (stderr) reach the user.
    pub fn stdout_showing_stderr(&self) -> Result<Vec<u8>, RunError> {
        let output = self.command().stderr(Stdio::inherit()).output().map_err(|e| self.spawn_error(e))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(RunError::Failed { command: self.line(), status: output.status })
        }
    }

    /// Run silently and capture stdout; `None` if it fails.
    pub fn output(&self) -> Result<Option<Vec<u8>>, RunError> {
        let output = self.command().stderr(Stdio::null()).output().map_err(|e| self.spawn_error(e))?;
        Ok(output.status.success().then_some(output.stdout))
    }
}

const KICAD_APP: &str = "/Applications/KiCad/KiCad.app/Contents";

/// Where the external tools are, from the environment or platform defaults.
#[derive(Clone, Debug)]
pub struct Tools {
    pub kicad_cli: PathBuf,
    pub footprints: PathBuf,
    pub java: PathBuf,
    pub magick: PathBuf,
    pub freerouting: PathBuf,
    /// Interpreter for the board project's external check scripts.
    pub python: PathBuf,
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn on_path(program: &str) -> Option<PathBuf> {
    which::which(program).ok()
}

impl Tools {
    pub fn from_env() -> Self {
        let kicad_cli = env_path("KICAD_CLI")
            .or_else(|| on_path("kicad-cli"))
            .unwrap_or_else(|| Path::new(KICAD_APP).join("MacOS/kicad-cli"));
        let footprints = env_path("KICAD_FOOTPRINT_DIR")
            .or_else(|| env_path("KICAD10_FOOTPRINT_DIR"))
            .unwrap_or_else(|| Path::new(KICAD_APP).join("SharedSupport/footprints"));
        let java = env_path("JAVA")
            .or_else(|| {
                ["/opt/homebrew/opt/openjdk/bin/java", "/usr/local/opt/openjdk/bin/java"]
                    .into_iter()
                    .map(PathBuf::from)
                    .find(|p| p.exists())
            })
            .or_else(|| on_path("java"))
            .unwrap_or_else(|| "java".into());
        let freerouting = env_path("FREEROUTING_JAR").unwrap_or_else(|| {
            env::home_dir().unwrap_or_default().join(".local/share/freerouting/freerouting-2.0.1.jar")
        });
        Self {
            kicad_cli,
            footprints,
            java,
            magick: env_path("MAGICK").or_else(|| on_path("magick")).unwrap_or_else(|| "magick".into()),
            freerouting,
            python: env_path("BB_PYTHON").or_else(|| on_path("python3")).unwrap_or_else(|| "python3".into()),
        }
    }

    pub fn kicad<I, S>(&self, args: I) -> Result<Cmd, RunError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Ok(Cmd::new(&self.kicad_cli).args(args))
    }

    /// `(name, path, found)` for each tool `doctor` reports on.
    pub fn inventory(&self) -> Vec<(&'static str, &Path, bool)> {
        let exists = |p: &Path| p.is_file() || which::which(p).is_ok();
        vec![
            ("kicad-cli", &self.kicad_cli, exists(&self.kicad_cli)),
            ("KiCad footprints", &self.footprints, self.footprints.is_dir()),
            ("java", &self.java, exists(&self.java)),
            ("FreeRouting", &self.freerouting, self.freerouting.is_file()),
            ("ImageMagick", &self.magick, exists(&self.magick)),
            ("python3", &self.python, exists(&self.python)),
        ]
    }
}
