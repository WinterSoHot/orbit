use serde_json::Value;
use std::{
    io::Write,
    process::{Child, ChildStdin},
};

// The leader is never reaped until its owned group is cleaned. This keeps the
// PGID reserved, including when the leader exits before its descendants.
pub(crate) struct OwnedChild {
    pub(crate) child: Child,
    pub(crate) stopped: bool,
    pub(crate) reaped: bool,
    pub(crate) group_killed: bool,
    pub(crate) stop_error: Option<String>,
}
impl OwnedChild {
    pub(crate) fn new(child: Child) -> Self {
        Self {
            child,
            stopped: false,
            reaped: false,
            group_killed: false,
            stop_error: None,
        }
    }
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        self.stop_with(|pid| {
            #[cfg(unix)]
            {
                // Immediate forced cleanup is intentional; user cancellation first
                // goes through turn/interrupt and awaits provider confirmation.
                if unsafe { libc::kill(-(pid as i32), libc::SIGKILL) } == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            }
            #[cfg(not(unix))]
            {
                let _ = pid;
                Ok(())
            }
        })
    }
    pub(crate) fn stop_with(
        &mut self,
        signal: impl FnOnce(u32) -> std::io::Result<()>,
    ) -> Result<(), String> {
        if self.reaped {
            return self.stop_error.clone().map_or(Ok(()), Err);
        }
        if !self.group_killed {
            if let Err(error) = signal(self.child.id()) {
                // Reaping disables future group signals, even when cleanup remains
                // unconfirmed. macOS can return EPERM for an exited zombie group.
                if matches!(self.child.try_wait(), Ok(Some(_))) {
                    self.reaped = true;
                }
                let message = format!("owned group cleanup unconfirmed: {error:?}");
                self.stop_error = Some(message.clone());
                return Err(message);
            }
            self.group_killed = true;
        }
        #[cfg(not(unix))]
        self.child
            .kill()
            .map_err(|error| format!("stop leader: {error:?}"))?;
        self.child
            .wait()
            .map_err(|error| format!("reap leader: {error:?}"))?;
        self.reaped = true;
        self.stopped = true;
        self.stop_error = None;
        Ok(())
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("Orbit: cleanup: {error}");
        }
    }
}
pub(crate) fn stop_child(child: &mut OwnedChild) {
    if let Err(error) = child.stop() {
        eprintln!("Orbit: cleanup: {error}");
    }
}
pub(crate) fn send(input: &mut ChildStdin, message: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *input, message).map_err(|_| "无法发送协议消息".to_string())?;
    input
        .write_all(b"\n")
        .and_then(|_| input.flush())
        .map_err(|_| "执行器连接已关闭".to_string())
}
