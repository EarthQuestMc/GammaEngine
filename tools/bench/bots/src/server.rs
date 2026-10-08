//! The server process of a run: started with its console on pipes, read line by line into a log
//! file and a channel, driven by commands written to its standard input (no RCON, no port),
//! stopped with `stop` and killed if it does not exit in time.
//!
//! Ctrl-C in the orchestrator's console does not reach the server: it runs in its own process
//! group, and the orchestrator stops it cleanly. On Windows it also belongs to a job object that
//! kills it when the orchestrator dies, so no server outlives a crashed or killed run.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::scenario::{Gc, ServerSpec};
use crate::signal;
use crate::util;

/// Console lines kept to show why a server failed.
const TAIL_LINES: usize = 40;
const POLL: Duration = Duration::from_millis(100);

/// `java -version` output and the major version (8 for `1.8.0_491`, 21 for `21.0.12`).
pub fn java_version(java: &str) -> Result<(String, u32), String> {
    let output = Command::new(java)
        .arg("-version")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {java} -version: {e}"))?;
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    let major = parse_java_major(&text)
        .ok_or_else(|| format!("cannot read the Java version from: {}", text.trim()))?;
    Ok((text, major))
}

fn parse_java_major(text: &str) -> Option<u32> {
    let start = text.find("version \"")? + "version \"".len();
    let version = &text[start..];
    let version = &version[..version.find('"')?];
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// The JVM and server arguments, in order; `nogui` stays last, where FML's parser expects it.
/// Same choices as `tools/test-server/start.ps1`.
pub fn jvm_arguments(
    spec: &ServerSpec,
    java_major: u32,
    java9args: &Path,
) -> Result<Vec<String>, String> {
    let mut args = vec![format!("-Xms{}", spec.heap), format!("-Xmx{}", spec.heap)];
    if java_major >= 9 {
        // The module system closes what Forge and the mods reach into: java9args.txt opens it.
        args.push(format!("@{}", java9args.display()));
    }
    match spec.gc {
        Gc::Default => {}
        Gc::G1 => args.push("-XX:+UseG1GC".into()),
        Gc::Zgc => {
            if java_major < 15 {
                return Err(format!(
                    "ZGC needs Java 15 or later; this is Java {java_major}"
                ));
            }
            args.push("-XX:+UseZGC".into());
            if (21..=22).contains(&java_major) {
                args.push("-XX:+ZGenerational".into());
            }
        }
    }
    if spec.gc_log {
        if java_major >= 9 {
            args.push("-Xlog:gc,safepoint:file=logs/gc-%p.log:uptime,level,tags".into());
        } else {
            args.extend([
                "-Xloggc:logs/gc-%p.log".to_owned(),
                "-XX:+PrintGCDetails".to_owned(),
                "-XX:+PrintGCApplicationStoppedTime".to_owned(),
            ]);
        }
    }
    args.extend(spec.jvm_args.iter().cloned());
    args.extend([
        "-jar".to_owned(),
        "server.jar".to_owned(),
        "nogui".to_owned(),
    ]);
    Ok(args)
}

#[derive(Debug, PartialEq)]
pub enum WaitError {
    Timeout,
    /// The server process ended; the text says how.
    Exited(String),
    Interrupted,
}

impl WaitError {
    pub fn describe(&self, what: &str, timeout: Duration) -> String {
        match self {
            WaitError::Timeout => format!("{what}: nothing after {} s", timeout.as_secs()),
            WaitError::Exited(how) => format!("{what}: the server {how}"),
            WaitError::Interrupted => format!("{what}: interrupted"),
        }
    }
}

pub struct ServerProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    readers: Vec<JoinHandle<()>>,
    log: Arc<Mutex<File>>,
    tail: VecDeque<String>,
    pub pid: u32,
    #[cfg(windows)]
    job: Option<job::Job>,
}

fn describe_exit(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exited with code {code}"),
        None => format!("ended ({status})"),
    }
}

impl ServerProcess {
    /// Starts `java args...` in `dir`, its console written to `console_log`.
    pub fn start(
        java: &str,
        args: &[String],
        dir: &Path,
        console_log: &Path,
    ) -> Result<ServerProcess, String> {
        let log = File::create(console_log)
            .map_err(|e| format!("cannot create {}: {e}", console_log.display()))?;
        let log = Arc::new(Mutex::new(log));
        let mut command = Command::new(java);
        command
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        detach_from_console_signals(&mut command);
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot start {java}: {e}"))?;
        #[cfg(windows)]
        let job = job::Job::kill_on_close().filter(|job| job.assign(&child));
        #[cfg(windows)]
        if job.is_none() {
            eprintln!(
                "warning: the server is not tied to this process: kill it by hand if the run dies"
            );
        }

        let (sender, receiver) = mpsc::channel();
        let mut readers = Vec::new();
        let stdout = child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>);
        let stderr = child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>);
        for (name, source) in [("server-stdout", stdout), ("server-stderr", stderr)] {
            let Some(source) = source else { continue };
            let (log, sender) = (Arc::clone(&log), sender.clone());
            let reader = thread::Builder::new()
                .name(name.into())
                .spawn(move || read_lines(source, &log, &sender))
                .map_err(|e| format!("cannot start the console reader: {e}"))?;
            readers.push(reader);
        }
        Ok(ServerProcess {
            pid: child.id(),
            stdin: child.stdin.take(),
            child,
            lines: receiver,
            readers,
            log,
            tail: VecDeque::new(),
            #[cfg(windows)]
            job,
        })
    }

    /// Writes one console command, and a `> command` line in the console log.
    pub fn send(&mut self, command: &str) -> Result<(), String> {
        println!("> {command}");
        if let Ok(mut log) = self.log.lock() {
            let _ = writeln!(log, "> {command}");
        }
        let stdin = self
            .stdin
            .as_mut()
            .ok_or("the server console is already closed")?;
        stdin
            .write_all(format!("{command}\n").as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("cannot write to the server console: {e}"))
    }

    fn remember(&mut self, line: String) {
        if self.tail.len() == TAIL_LINES {
            self.tail.pop_front();
        }
        self.tail.push_back(line);
    }

    /// Takes the lines read so far; returns how the server ended if it did.
    pub fn poll(&mut self) -> Option<String> {
        while let Ok(line) = self.lines.try_recv() {
            self.remember(line);
        }
        match self.child.try_wait() {
            Ok(Some(status)) => Some(describe_exit(status)),
            Ok(None) => None,
            Err(e) => Some(format!("cannot be watched: {e}")),
        }
    }

    /// Waits for a console line (colours removed) that `matches` accepts, and returns it.
    pub fn wait_for(
        &mut self,
        timeout: Duration,
        matches: impl Fn(&str) -> bool,
    ) -> Result<String, WaitError> {
        let deadline = Instant::now() + timeout;
        loop {
            if signal::interrupted() {
                return Err(WaitError::Interrupted);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(WaitError::Timeout);
            }
            match self.lines.recv_timeout(left.min(POLL)) {
                Ok(line) => {
                    let found = matches(&line);
                    self.remember(line.clone());
                    if found {
                        return Ok(line);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Ok(Some(status)) = self.child.try_wait() {
                        // The readers may still hold the last lines: let them finish first.
                        self.join_readers();
                        while let Ok(line) = self.lines.try_recv() {
                            if matches(&line) {
                                return Ok(line);
                            }
                            self.remember(line);
                        }
                        return Err(WaitError::Exited(describe_exit(status)));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let status = self
                        .child
                        .wait()
                        .map_or_else(|e| format!("cannot be watched: {e}"), describe_exit);
                    return Err(WaitError::Exited(status));
                }
            }
        }
    }

    /// The last console lines, to show why something failed.
    pub fn tail(&self) -> impl Iterator<Item = &str> {
        self.tail.iter().map(String::as_str)
    }

    fn join_readers(&mut self) {
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }

    /// Sends `stop`, closes the console and waits for the process to exit; kills it after
    /// `timeout`. Returns how it ended.
    pub fn shutdown(mut self, timeout: Duration) -> String {
        if let Some(how) = self.poll() {
            self.join_readers();
            return how;
        }
        let _ = self.send("stop");
        // End of input also ends the console reader thread of the server.
        self.stdin = None;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(how) = self.poll() {
                self.join_readers();
                return how;
            }
            if Instant::now() >= deadline {
                self.kill();
                return format!("killed after {} s without exiting", timeout.as_secs());
            }
            thread::sleep(POLL);
        }
    }

    /// Kills the server and whatever it started. The `java` of the PATH on Windows is often a
    /// launcher (`Common Files\Oracle\Java\java8path\java.exe`) that runs the real JVM as its
    /// child: killing the launcher alone would leave the JVM running, holding the console pipes.
    fn kill(&mut self) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
        }
        #[cfg(unix)]
        unix::kill_group(self.pid);
        let _ = self.child.kill();
        let _ = self.child.wait();
        // The readers end when the last process holding the pipes is gone; not waited for.
        self.readers.clear();
    }
}

impl Drop for ServerProcess {
    /// Last resort on an error path: never leave a server running.
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            eprintln!("killing the server (pid {})", self.pid);
            self.kill();
        }
    }
}

#[cfg(unix)]
mod unix {
    const SIGKILL: i32 = 9;

    extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }

    /// The server leads its own process group (`process_group(0)`): kill the whole group.
    pub fn kill_group(leader: u32) {
        if let Ok(pid) = i32::try_from(leader) {
            // SAFETY: kill(2) with a negative pid signals the process group; no memory involved.
            unsafe {
                kill(-pid, SIGKILL);
            }
        }
    }
}

fn read_lines(source: Box<dyn Read + Send>, log: &Mutex<File>, lines: &Sender<String>) {
    let mut reader = BufReader::new(source);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let _ = log
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .write_all(&buf);
                let text = String::from_utf8_lossy(&buf);
                let line = util::plain_text(text.trim_end_matches(['\r', '\n']));
                // The receiver is gone once the run is over; the log still gets every line.
                let _ = lines.send(line);
            }
        }
    }
}

#[cfg(windows)]
fn detach_from_console_signals(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // CREATE_NEW_PROCESS_GROUP: Ctrl-C in this console is not delivered to the server.
    command.creation_flags(0x0000_0200);
}

#[cfg(unix)]
fn detach_from_console_signals(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // Its own process group: the terminal's SIGINT does not reach the server.
    command.process_group(0);
}

#[cfg(not(any(windows, unix)))]
fn detach_from_console_signals(_command: &mut Command) {}

/// A job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: when the orchestrator exits, however
/// it exits, the system closes the handle and kills the server.
#[cfg(windows)]
mod job {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    type Handle = *mut c_void;

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimitInformation {
        basic: BasicLimitInformation,
        io_counters: [u64; 6],
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(job: Handle, class: i32, info: *mut c_void, length: u32) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    pub struct Job(Handle);

    impl Job {
        pub fn kill_on_close() -> Option<Job> {
            // SAFETY: plain Win32 calls on a handle this function owns; the structure has the
            // C layout of JOBOBJECT_EXTENDED_LIMIT_INFORMATION.
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let job = Job(handle);
                let mut info = ExtendedLimitInformation::default();
                info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    handle,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                    &mut info as *mut ExtendedLimitInformation as *mut c_void,
                    std::mem::size_of::<ExtendedLimitInformation>() as u32,
                );
                (ok != 0).then_some(job)
            }
        }

        pub fn assign(&self, child: &Child) -> bool {
            // SAFETY: both handles are valid for the duration of the call.
            unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as Handle) != 0 }
        }

        /// Kills every process of the job, the server's children included.
        pub fn terminate(&self) {
            // SAFETY: the handle is owned by this value and still open.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle is owned by this value and closed once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn layout_matches_windows() {
            // sizeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION) on 64-bit Windows.
            #[cfg(target_pointer_width = "64")]
            assert_eq!(std::mem::size_of::<super::ExtendedLimitInformation>(), 144);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spec() -> ServerSpec {
        ServerSpec {
            dir: PathBuf::from("test-server-orch"),
            jar: None,
            java: "java".into(),
            heap: "1G".into(),
            gc: Gc::Default,
            gc_log: false,
            jvm_args: vec![],
            port: 25570,
            world: None,
            world_sha256: None,
            startup_timeout: Duration::from_secs(300),
            stop_timeout: Duration::from_secs(120),
        }
    }

    #[test]
    fn java_major() {
        assert_eq!(
            parse_java_major("java version \"1.8.0_491\"\nJava(TM) SE Runtime"),
            Some(8)
        );
        assert_eq!(
            parse_java_major("openjdk version \"21.0.12\" 2026-07-21 LTS"),
            Some(21)
        );
        assert_eq!(parse_java_major("openjdk version \"25-ea\""), Some(25));
        assert_eq!(parse_java_major("no version here"), None);
    }

    #[test]
    fn arguments_java_8() {
        let mut s = spec();
        s.gc_log = true;
        s.jvm_args = vec!["-XX:+AlwaysPreTouch".into()];
        let args = jvm_arguments(&s, 8, Path::new("java9args.txt")).unwrap();
        assert_eq!(
            args,
            [
                "-Xms1G",
                "-Xmx1G",
                "-Xloggc:logs/gc-%p.log",
                "-XX:+PrintGCDetails",
                "-XX:+PrintGCApplicationStoppedTime",
                "-XX:+AlwaysPreTouch",
                "-jar",
                "server.jar",
                "nogui"
            ]
        );
        s.gc = Gc::Zgc;
        assert!(jvm_arguments(&s, 8, Path::new("x")).is_err());
    }

    #[test]
    fn arguments_java_21() {
        let mut s = spec();
        s.gc = Gc::Zgc;
        s.heap = "2G".into();
        let args = jvm_arguments(&s, 21, Path::new("/repo/java9args.txt")).unwrap();
        assert_eq!(
            args,
            [
                "-Xms2G",
                "-Xmx2G",
                "@/repo/java9args.txt",
                "-XX:+UseZGC",
                "-XX:+ZGenerational",
                "-jar",
                "server.jar",
                "nogui"
            ]
        );
        s.gc = Gc::G1;
        let args = jvm_arguments(&s, 25, Path::new("a")).unwrap();
        assert!(args.contains(&"-XX:+UseG1GC".to_owned()));
        assert_eq!(args.last().map(String::as_str), Some("nogui"));
    }
}
