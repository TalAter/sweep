//! Keep Sweep alive long enough to record a shell terminated by a signal.
use std::{
    fs::{File, OpenOptions},
    io,
    os::fd::AsRawFd,
    os::unix::process::{CommandExt, ExitStatusExt},
    process::{Command, ExitStatus},
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicI32, Ordering},
    },
};

static EXECUTION: Mutex<()> = Mutex::new(());
static CHILD: AtomicI32 = AtomicI32::new(0);
static RECEIVED: AtomicI32 = AtomicI32::new(0);

extern "C" fn forward(signal: libc::c_int) {
    RECEIVED.store(signal, Ordering::SeqCst);
    let pid = CHILD.load(Ordering::SeqCst);
    if pid > 0 {
        // SAFETY: kill is async-signal-safe; only target the registered child.
        unsafe { libc::kill(-pid, signal) };
    }
}

pub(crate) struct ExecutionSignals {
    previous: Vec<(libc::c_int, libc::sigaction)>,
    terminal: Option<(File, libc::pid_t)>,
    _exclusive: MutexGuard<'static, ()>,
}

impl ExecutionSignals {
    pub(crate) fn open() -> io::Result<Self> {
        let exclusive = EXECUTION
            .lock()
            .map_err(|_| io::Error::other("execution lock poisoned"))?;
        CHILD.store(0, Ordering::SeqCst);
        RECEIVED.store(0, Ordering::SeqCst);
        let terminal = match OpenOptions::new().read(true).write(true).open("/dev/tty") {
            Ok(file) => {
                let group = unsafe { libc::tcgetpgrp(file.as_raw_fd()) };
                if group == -1 {
                    return Err(io::Error::last_os_error());
                }
                Some((file, group))
            }
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(libc::ENXIO | libc::ENODEV | libc::ENOENT)
                ) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        let mut guard = Self {
            terminal,
            previous: Vec::with_capacity(3),
            _exclusive: exclusive,
        };
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            // SAFETY: sigaction initializes the old disposition; new action has
            // a valid handler and an empty mask before registration.
            let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
            if unsafe { libc::sigaction(signal, std::ptr::null(), &mut previous) } == -1 {
                return Err(io::Error::last_os_error());
            }
            // Preserve signals the invoking environment deliberately ignored.
            if previous.sa_sigaction == libc::SIG_IGN {
                continue;
            }
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = forward as *const () as libc::sighandler_t;
            action.sa_flags = libc::SA_RESTART;
            unsafe { libc::sigemptyset(&mut action.sa_mask) };
            if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } == -1 {
                return Err(io::Error::last_os_error());
            }
            guard.previous.push((signal, previous));
        }
        Ok(guard)
    }

    pub(crate) fn prepare(&self, process: &mut Command) {
        process.process_group(0);
        if let Some((terminal, _)) = &self.terminal {
            let fd = terminal.as_raw_fd();
            // SAFETY: the child callback only performs async-signal-safe syscalls.
            // Command sets the process group before invoking pre_exec.
            unsafe {
                process.pre_exec(move || set_foreground(fd, libc::getpgrp()));
            }
        }
    }

    pub(crate) fn restore_spawn_failure(&self) -> io::Result<()> {
        // pre_exec may have handed over the tty before exec reported an error;
        // spawn then provides no child PID to register for normal restoration.
        if let Some((terminal, group)) = &self.terminal {
            set_foreground(terminal.as_raw_fd(), *group)?;
        }
        Ok(())
    }

    pub(crate) fn restore_terminal(&self) -> io::Result<()> {
        if let Some((terminal, group)) = &self.terminal {
            let foreground = unsafe { libc::tcgetpgrp(terminal.as_raw_fd()) };
            if foreground == -1 {
                return Err(io::Error::last_os_error());
            }
            // After `bg`, an unrelated shell may own the terminal. Never take it.
            if foreground == CHILD.load(Ordering::SeqCst) {
                set_foreground(terminal.as_raw_fd(), *group)?;
            }
        }
        Ok(())
    }

    pub(crate) fn wait(&self, pid: u32) -> io::Result<ExitStatus> {
        loop {
            let mut status = 0;
            // Observe job-control stops while another thread feeds the script.
            let result = unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WUNTRACED) };
            if result == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if libc::WIFSTOPPED(status) {
                self.restore_terminal()?;
                // Suspend Sweep too so its invoking shell can regain job control.
                // SIGSTOP also works in an orphaned process group (PTY harnesses).
                unsafe { libc::raise(libc::SIGSTOP) };
                if let Some((terminal, _)) = &self.terminal {
                    let foreground = unsafe { libc::tcgetpgrp(terminal.as_raw_fd()) };
                    if foreground == -1 {
                        return Err(io::Error::last_os_error());
                    }
                    // `fg` gives Sweep the terminal; `bg` deliberately does not.
                    if foreground == unsafe { libc::getpgrp() } {
                        set_foreground(terminal.as_raw_fd(), pid as libc::pid_t)?;
                    }
                }
                unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGCONT) };
            } else {
                return Ok(ExitStatus::from_raw(status));
            }
        }
    }

    pub(crate) fn set_child(&self, pid: u32) {
        CHILD.store(pid as libc::pid_t, Ordering::SeqCst);
        let signal = RECEIVED.load(Ordering::SeqCst);
        if signal != 0 {
            // A signal received between registration and spawn must reach the child.
            unsafe { libc::kill(-(pid as libc::pid_t), signal) };
        }
    }

    pub(crate) fn clear_child(&self) {
        CHILD.store(0, Ordering::SeqCst);
    }
}

impl Drop for ExecutionSignals {
    fn drop(&mut self) {
        let _ = self.restore_terminal();
        self.clear_child();
        for (signal, previous) in self.previous.iter().rev() {
            // SAFETY: restore dispositions returned by sigaction registration.
            unsafe { libc::sigaction(*signal, previous, std::ptr::null_mut()) };
        }
    }
}

// tcsetpgrp from a background group normally stops the caller with SIGTTOU.
// Block it on this thread only, both during child handoff and parent restoration.
fn set_foreground(fd: libc::c_int, group: libc::pid_t) -> io::Result<()> {
    unsafe {
        let mut mask: libc::sigset_t = std::mem::zeroed();
        let mut previous: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        libc::sigaddset(&mut mask, libc::SIGTTOU);
        if libc::sigprocmask(libc::SIG_BLOCK, &mask, &mut previous) == -1 {
            return Err(io::Error::last_os_error());
        }
        let result = if libc::tcsetpgrp(fd, group) == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        };
        libc::sigprocmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut());
        result
    }
}
