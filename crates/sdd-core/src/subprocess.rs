//! 外部命令的统一有界执行器。
//!
//! 所有外部 CLI 共用同一套安全边界：关闭 stdin、独立进程组、总时限、单流输出上限、
//! 完整管道校验与后代进程清理。调用方只负责把 I/O 错误转换为自己的领域错误。

use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const PIPE_CLOSE_GRACE: Duration = Duration::from_secs(1);

pub(crate) fn run_command(
    program: &Path,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
    environment: &[(&str, &str)],
) -> io::Result<Output> {
    let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "外部命令超时时间超出可表示范围",
        )
    })?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .envs(environment.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);

    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("stdout 已配置为 piped");
    let stderr = child.stderr.take().expect("stderr 已配置为 piped");
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    let stdout_reader = std::thread::spawn(move || pump_pipe(stdout, stdout_tx));
    let stderr_reader = std::thread::spawn(move || pump_pipe(stderr, stderr_tx));

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let mut stdout_state = PipeState::default();
    let mut stderr_state = PipeState::default();
    let exit_status;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_status = status;
                break;
            }
            Ok(None) => {}
            Err(error) => {
                terminate_and_reap(&mut child);
                return Err(error);
            }
        }
        if let Err(error) = drain_available(&stdout_rx, &mut stdout_buf, &mut stdout_state)
            .and_then(|()| drain_available(&stderr_rx, &mut stderr_buf, &mut stderr_state))
        {
            terminate_and_reap(&mut child);
            return Err(error);
        }
        let now = Instant::now();
        if now >= deadline {
            terminate_and_reap(&mut child);
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "命令 {} 执行超时（{}ms）",
                    program.display(),
                    timeout.as_millis()
                ),
            ));
        }
        std::thread::sleep((deadline - now).min(POLL_INTERVAL));
    }

    if let Err(error) = drain_available(&stdout_rx, &mut stdout_buf, &mut stdout_state)
        .and_then(|()| drain_available(&stderr_rx, &mut stderr_buf, &mut stderr_state))
    {
        kill_process_group(&mut child);
        return Err(error);
    }
    if !stdout_state.done || !stderr_state.done {
        // 主进程已经退出但后代仍持有管道时，终止整个进程组再给 reader 一个短回收窗口。
        kill_process_group(&mut child);
        let grace_deadline = Instant::now()
            .checked_add(PIPE_CLOSE_GRACE)
            .expect("固定管道回收窗口必须可表示")
            .min(deadline);
        drain_pipe_until(
            &stdout_rx,
            &mut stdout_buf,
            &mut stdout_state,
            grace_deadline,
        )?;
        drain_pipe_until(
            &stderr_rx,
            &mut stderr_buf,
            &mut stderr_state,
            grace_deadline,
        )?;
    }
    validate_pipe_state("stdout", stdout_state)?;
    validate_pipe_state("stderr", stderr_state)?;
    stdout_reader
        .join()
        .map_err(|_| io::Error::other("stdout 读取线程异常退出"))?;
    stderr_reader
        .join()
        .map_err(|_| io::Error::other("stderr 读取线程异常退出"))?;

    Ok(Output {
        status: exit_status,
        stdout: stdout_buf,
        stderr: stderr_buf,
    })
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn configure_process_group(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0000_0200);
}

#[cfg(not(any(unix, windows)))]
fn configure_process_group(_command: &mut Command) {}

#[derive(Debug)]
enum PipeEvent {
    Data(Vec<u8>),
    LimitExceeded,
    ReadFailed(io::Error),
}

#[derive(Debug, Default, Clone, Copy)]
struct PipeState {
    done: bool,
    limit_exceeded: bool,
}

fn pump_pipe(mut pipe: impl Read, tx: mpsc::Sender<PipeEvent>) {
    const CHUNK_SIZE: usize = 8192;
    let mut buffer = [0_u8; CHUNK_SIZE];
    let mut total = 0usize;
    let mut limit_reported = false;
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                let retained = count.min(MAX_OUTPUT_BYTES - total);
                if retained > 0 {
                    if tx
                        .send(PipeEvent::Data(buffer[..retained].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                    total += retained;
                }
                if retained < count && !limit_reported {
                    limit_reported = true;
                    if tx.send(PipeEvent::LimitExceeded).is_err() {
                        break;
                    }
                }
            }
            Err(error) => {
                drop(tx.send(PipeEvent::ReadFailed(error)));
                break;
            }
        }
    }
}

fn apply_pipe_event(
    event: PipeEvent,
    buffer: &mut Vec<u8>,
    state: &mut PipeState,
) -> io::Result<()> {
    match event {
        PipeEvent::Data(chunk) => buffer.extend_from_slice(&chunk),
        PipeEvent::LimitExceeded => {
            state.limit_exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                format!("外部命令输出超过 {MAX_OUTPUT_BYTES} 字节上限"),
            ));
        }
        PipeEvent::ReadFailed(error) => return Err(error),
    }
    Ok(())
}

fn drain_available(
    receiver: &mpsc::Receiver<PipeEvent>,
    buffer: &mut Vec<u8>,
    state: &mut PipeState,
) -> io::Result<()> {
    loop {
        match receiver.try_recv() {
            Ok(event) => apply_pipe_event(event, buffer, state)?,
            Err(mpsc::TryRecvError::Empty) => return Ok(()),
            Err(mpsc::TryRecvError::Disconnected) => {
                state.done = true;
                return Ok(());
            }
        }
    }
}

fn drain_pipe_until(
    receiver: &mpsc::Receiver<PipeEvent>,
    buffer: &mut Vec<u8>,
    state: &mut PipeState,
    deadline: Instant,
) -> io::Result<()> {
    while !state.done {
        let now = Instant::now();
        if now >= deadline {
            return Ok(());
        }
        match receiver.recv_timeout(deadline - now) {
            Ok(event) => apply_pipe_event(event, buffer, state)?,
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Disconnected) => state.done = true,
        }
    }
    Ok(())
}

fn validate_pipe_state(stream: &str, state: PipeState) -> io::Result<()> {
    if state.limit_exceeded {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            format!("{stream} 输出超过 {MAX_OUTPUT_BYTES} 字节上限"),
        ));
    }
    if !state.done {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("{stream} 管道未在子进程退出后及时关闭"),
        ));
    }
    Ok(())
}

fn terminate_and_reap(child: &mut std::process::Child) {
    kill_process_group(child);
    drop(child.wait());
}

/// 终止整个进程组，并以主进程 kill 作为进程组命令失败时的兜底清理。
fn kill_process_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        const SIGKILL: i32 = 9;
        if let Ok(pid) = i32::try_from(child.id()) {
            // SAFETY: pid 来自刚创建的子进程；负 pid 只定位该子进程创建的独立进程组。
            let _kill_status = unsafe { kill(-pid, SIGKILL) };
        }
        drop(child.kill());
    }
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        drop(
            Command::new("taskkill")
                .args(["/PID", pid.as_str(), "/T", "/F"])
                .status(),
        );
        drop(child.kill());
    }
    #[cfg(not(any(unix, windows)))]
    drop(child.kill());
}

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("读取失败"))
        }
    }

    #[test]
    fn pipe_retains_output_up_to_the_cap_boundary() {
        let mut input = vec![b'a'; MAX_OUTPUT_BYTES - 1];
        input.extend(*b"bc");
        let (sender, receiver) = mpsc::channel();
        pump_pipe(std::io::Cursor::new(input), sender);

        let mut output = Vec::new();
        let mut state = PipeState::default();
        let error = drain_pipe_until(
            &receiver,
            &mut output,
            &mut state,
            Instant::now() + Duration::from_millis(10),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
        drain_pipe_until(
            &receiver,
            &mut output,
            &mut state,
            Instant::now() + Duration::from_millis(10),
        )
        .unwrap();
        assert!(state.done);
        assert!(state.limit_exceeded);
        assert_eq!(output.len(), MAX_OUTPUT_BYTES);
        assert_eq!(output[MAX_OUTPUT_BYTES - 1], b'b');
    }

    #[test]
    fn pipe_read_error_is_not_treated_as_eof() {
        let (sender, receiver) = mpsc::channel();
        pump_pipe(FailingReader, sender);

        let error = drain_pipe_until(
            &receiver,
            &mut Vec::new(),
            &mut PipeState::default(),
            Instant::now() + Duration::from_millis(10),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "读取失败");
    }

    #[test]
    fn invalid_deadline_is_rejected_before_starting_the_command() {
        let error = run_command(
            Path::new("__sdd_missing__/command"),
            &[],
            Path::new("."),
            Duration::MAX,
            &[],
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn command_does_not_wait_for_descendants_holding_pipes() {
        let started = Instant::now();
        let output = run_command(
            Path::new("/bin/sh"),
            &["-c", "printf done; (sleep 10) & exit 0"],
            Path::new("."),
            Duration::from_secs(2),
            &[],
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"done");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn command_stops_immediately_when_output_exceeds_cap() {
        let started = Instant::now();
        let error = run_command(
            Path::new("/bin/sh"),
            &["-c", "yes output"],
            Path::new("."),
            Duration::from_secs(5),
            &[],
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
