use anyhow::Result;
use matiane_core::process::{
    AlwaysCommandOptions, RunningHandle, run_always_command,
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub trait ToCommand {
    fn to_command(self) -> Vec<String>;
}

#[derive(Default)]
pub struct SwayIdle {
    args: Vec<String>,
}

impl SwayIdle {
    pub fn new() -> Self {
        SwayIdle::default()
    }

    pub fn add_command(&mut self, cmd: impl ToCommand) -> &mut Self {
        self.args.append(&mut cmd.to_command());
        self
    }

    pub fn spawn(self, cancel: CancellationToken) -> Result<RunningHandle> {
        let opts = AlwaysCommandOptions {
            name: "swayidle".to_string(),
            args: self.args,
            restart_delay: Duration::from_millis(100),
        };

        if opts.args.is_empty() {
            return Err(anyhow::anyhow!(
                "Can not run swayidle with zero args!"
            ));
        }

        Ok(run_always_command(opts, cancel))
    }
}

pub struct Timeout {
    pub timeout: u32,
    pub timeout_cmd: String,

    pub resume: Option<String>,
}

impl Timeout {
    pub fn new(command: String, timeout: u32) -> Self {
        Timeout {
            timeout,
            timeout_cmd: command,
            resume: None,
        }
    }

    pub fn new_with_resume(
        command: String,
        timeout: u32,
        resume: String,
    ) -> Self {
        Timeout {
            timeout,
            timeout_cmd: command,
            resume: Some(resume),
        }
    }
}

impl ToCommand for Timeout {
    fn to_command(self) -> Vec<String> {
        let mut args: Vec<String> =
            vec!["timeout".into(), self.timeout.to_string(), self.timeout_cmd];

        if let Some(command) = self.resume {
            args.push("resume".into());
            args.push(command);
        }

        args
    }
}

pub struct BeforeSleep {
    pub command: String,
}

impl BeforeSleep {
    pub fn new(command: String) -> Self {
        BeforeSleep { command }
    }
}

impl ToCommand for BeforeSleep {
    fn to_command(self) -> Vec<String> {
        vec!["before-sleep".to_string(), self.command]
    }
}

pub struct AfterResume {
    pub command: String,
}

impl AfterResume {
    pub fn new(command: String) -> Self {
        AfterResume { command }
    }
}

impl ToCommand for AfterResume {
    fn to_command(self) -> Vec<String> {
        vec!["after-resume".to_string(), self.command]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swayidle_before_sleep_cmd_test() {
        let mut swayidle = SwayIdle::new();

        swayidle.add_command(BeforeSleep::new("dothisbeforesleep".into()));

        assert_eq!(swayidle.args.len(), 2);
        assert_eq!(swayidle.args[0], "before-sleep");
        assert_eq!(swayidle.args[1], "dothisbeforesleep");
    }

    #[test]
    fn swayidle_after_sleep_cmd_test() {
        let mut swayidle = SwayIdle::new();

        swayidle.add_command(AfterResume::new("dothisafterresume".into()));

        assert_eq!(swayidle.args.len(), 2);
        assert_eq!(swayidle.args[0], "after-resume");
        assert_eq!(swayidle.args[1], "dothisafterresume");
    }

    #[test]
    fn swayidle_timeout_cmd_test() {
        let mut swayidle = SwayIdle::new();

        swayidle.add_command(Timeout::new("timeoutcmd".into(), 100));

        assert_eq!(swayidle.args.len(), 3);
        assert_eq!(swayidle.args[0], "timeout");
        assert_eq!(swayidle.args[1], "100");
        assert_eq!(swayidle.args[2], "timeoutcmd");

        swayidle.add_command(Timeout::new_with_resume(
            "timeout2cmd".into(),
            20,
            "onresumecmd".into(),
        ));

        assert_eq!(swayidle.args.len(), 3 + 3 + 2);
        assert_eq!(swayidle.args[3], "timeout");
        assert_eq!(swayidle.args[4], "20");
        assert_eq!(swayidle.args[5], "timeout2cmd");
        assert_eq!(swayidle.args[6], "resume");
        assert_eq!(swayidle.args[7], "onresumecmd");
    }
}
