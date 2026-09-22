use super::headless::apply_std_headless_flags;
use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

fn provider_home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub(crate) fn find_executable(name: &str) -> Option<PathBuf> {
    if name == "cursor" {
        return find_named_executable("agent").or_else(|| find_named_executable("cursor-agent"));
    }
    if name == "antigravity" {
        return find_named_executable("agy").or_else(|| find_named_executable("antigravity"));
    }
    find_named_executable(name)
}

fn find_named_executable(name: &str) -> Option<PathBuf> {
    let search_path = current_provider_environment_path();
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for lookup in executable_lookup_names(name) {
        if let Ok(path) = which::which_in(&lookup, Some(search_path), &current_dir) {
            return Some(path);
        }
    }
    let home = provider_home_dir();
    let mut candidates = Vec::new();
    for lookup in executable_lookup_names(name) {
        candidates.push(PathBuf::from("/opt/homebrew/bin").join(&lookup));
        candidates.push(PathBuf::from("/usr/local/bin").join(&lookup));
        if let Some(home) = home.as_ref() {
            candidates.push(home.join(".local/bin").join(&lookup));
            candidates.push(home.join(".codex/bin").join(&lookup));
        }
        #[cfg(windows)]
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            let local = PathBuf::from(local_app_data);
            candidates.push(local.join("cursor-agent").join(&lookup));
            candidates.push(local.join("agy").join("bin").join(&lookup));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

pub(crate) fn executable_lookup_names(name: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        if let Some((stem, ext)) = name.rsplit_once('.') {
            if ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("cmd") {
                return vec![name.to_string()];
            }
            return vec![
                name.to_string(),
                format!("{stem}.exe"),
                format!("{stem}.cmd"),
            ];
        }
        vec![
            name.to_string(),
            format!("{name}.exe"),
            format!("{name}.cmd"),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![name.to_string()]
    }
}

pub(crate) fn merge_provider_paths(
    inherited: Option<&OsStr>,
    login_shell: Option<&OsStr>,
    home: Option<&Path>,
) -> OsString {
    let mut entries = Vec::<PathBuf>::new();
    let mut append = |value: &OsStr| {
        for path in env::split_paths(value) {
            if !entries.contains(&path) {
                entries.push(path);
            }
        }
    };
    if let Some(path) = login_shell {
        append(path);
    }
    if let Some(path) = inherited {
        append(path);
    }
    if let Some(home) = home {
        let local_bin = home.join(".local/bin");
        if !entries.contains(&local_bin) {
            entries.push(local_bin);
        }
    }
    #[cfg(windows)]
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        let root = PathBuf::from(local_app_data);
        for extra in [root.join("cursor-agent"), root.join("agy").join("bin")] {
            if !entries.contains(&extra) {
                entries.push(extra);
            }
        }
    }
    env::join_paths(entries).unwrap_or_else(|_| inherited.unwrap_or_default().to_os_string())
}

const PROVIDER_PATH_MARKER: &str = "__SPRITE_STUDIO_PATH__=";

pub(crate) fn login_shell_path(shell: &Path) -> Option<OsString> {
    login_shell_path_with_timeout(shell, Duration::from_secs(3))
}

#[cfg(unix)]
pub(crate) fn isolate_login_shell(command: &mut StdCommand) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
pub(crate) fn isolate_login_shell(_command: &mut StdCommand) {}

#[cfg(unix)]
fn kill_login_shell_group(pid: u32, observed_exit: bool) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: the probe is placed in a dedicated process group whose id is the
    // direct child's pid. A negative pid asks kill(2) to signal that group.
    let result = unsafe { libc::kill(-pid, libc::SIGKILL) };
    let error = std::io::Error::last_os_error();
    result == 0
        || error.raw_os_error() == Some(libc::ESRCH)
        // macOS reports EPERM when WNOWAIT has confirmed that the group
        // contains only the unreaped zombie leader and no signalable members.
        || (observed_exit && error.raw_os_error() == Some(libc::EPERM))
}

#[cfg(not(unix))]
fn kill_login_shell_group(_pid: u32, _observed_exit: bool) -> bool {
    true
}

pub(crate) enum LoginShellState {
    Running,
    Exited,
    Error,
}

#[cfg(unix)]
pub(crate) fn login_shell_state(child: &mut std::process::Child) -> LoginShellState {
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    // SAFETY: info points to writable siginfo_t storage, and WNOWAIT observes
    // the child without reaping it so its PID/process-group id cannot be reused.
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            child.id() as libc::id_t,
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result != 0 {
        return LoginShellState::Error;
    }
    // SAFETY: waitid initialized info on success. A zero pid with WNOHANG means
    // the child has not changed state yet.
    let pid = unsafe { info.assume_init().si_pid() };
    if pid == 0 {
        LoginShellState::Running
    } else {
        LoginShellState::Exited
    }
}

#[cfg(not(unix))]
pub(crate) fn login_shell_state(child: &mut std::process::Child) -> LoginShellState {
    match child.try_wait() {
        Ok(Some(_)) => LoginShellState::Exited,
        Ok(None) => LoginShellState::Running,
        Err(_) => LoginShellState::Error,
    }
}

pub(crate) fn stop_login_shell(
    child: &mut std::process::Child,
    observed_exit: bool,
) -> Option<std::process::ExitStatus> {
    if !kill_login_shell_group(child.id(), observed_exit) {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    let _ = child.kill();
    child.wait().ok()
}

pub(crate) fn login_shell_path_with_timeout(shell: &Path, timeout: Duration) -> Option<OsString> {
    let output_path = env::temp_dir().join(format!("sprite-studio-path-{}.txt", Uuid::new_v4()));
    let result = (|| {
        let output_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output_path)
            .ok()?;
        let mut command = StdCommand::new(shell);
        command
            .args(["-ilc", "printf '\\n__SPRITE_STUDIO_PATH__=%s\\n' \"$PATH\""])
            .stdout(Stdio::from(output_file))
            .stderr(Stdio::null());
        apply_std_headless_flags(&mut command);
        isolate_login_shell(&mut command);
        let mut child = command.spawn().ok()?;
        let deadline = Instant::now() + timeout;
        let status = loop {
            match login_shell_state(&mut child) {
                LoginShellState::Exited => break stop_login_shell(&mut child, true)?,
                LoginShellState::Running => {}
                LoginShellState::Error => {
                    let _ = stop_login_shell(&mut child, false);
                    return None;
                }
            }
            if Instant::now() >= deadline {
                let _ = stop_login_shell(&mut child, false);
                return None;
            }
            thread::sleep(
                Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())),
            );
        };
        if !status.success() {
            return None;
        }
        let stdout = fs::read(&output_path).ok()?;

        let stdout = String::from_utf8(stdout).ok()?;
        stdout.lines().rev().find_map(|line| {
            line.strip_prefix(PROVIDER_PATH_MARKER)
                .filter(|path| !path.is_empty())
                .map(OsString::from)
        })
    })();
    let _ = fs::remove_file(output_path);
    result
}

pub(crate) fn provider_environment_path(
    inherited: Option<&OsStr>,
    shell: Option<&Path>,
    home: Option<&Path>,
) -> OsString {
    let login_shell = shell.and_then(login_shell_path);
    merge_provider_paths(inherited, login_shell.as_deref(), home)
}

pub(crate) fn current_provider_environment_path() -> &'static OsString {
    static PATH: OnceLock<OsString> = OnceLock::new();
    PATH.get_or_init(|| {
        let inherited = env::var_os("PATH");
        let home = provider_home_dir();
        #[cfg(not(windows))]
        let shell = env::var_os("SHELL")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                ["/bin/zsh", "/bin/bash", "/bin/sh"]
                    .into_iter()
                    .map(PathBuf::from)
                    .find(|path| path.is_file())
            });
        #[cfg(windows)]
        let shell: Option<PathBuf> = None;
        provider_environment_path(inherited.as_deref(), shell.as_deref(), home.as_deref())
    })
}

pub(crate) fn provider_process_path(provider_id: &str) -> OsString {
    let inherited = current_provider_environment_path().clone();
    #[cfg(windows)]
    if provider_id == "cursor" {
        return prepend_git_bash_directories(inherited);
    }
    #[cfg(not(windows))]
    let _ = provider_id;
    inherited
}

#[cfg(windows)]
fn prepend_git_bash_directories(inherited: OsString) -> OsString {
    let mut extras = Vec::new();
    for root in [env::var_os("ProgramW6432"), env::var_os("ProgramFiles")]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
    {
        let bin = root.join("Git").join("bin");
        let usr_bin = root.join("Git").join("usr").join("bin");
        if bin.is_dir() && !extras.contains(&bin) {
            extras.push(bin);
        }
        if usr_bin.is_dir() && !extras.contains(&usr_bin) {
            extras.push(usr_bin);
        }
    }
    if extras.is_empty() {
        return inherited;
    }
    let mut entries = extras;
    for path in env::split_paths(&inherited) {
        if !entries.contains(&path) {
            entries.push(path);
        }
    }
    env::join_paths(entries).unwrap_or(inherited)
}
