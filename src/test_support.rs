#[cfg(unix)]
use std::os::fd::AsRawFd;

pub(crate) struct EnvLockGuard {
    #[cfg(unix)]
    file: std::fs::File,
}

pub(crate) fn env_lock() -> EnvLockGuard {
    #[cfg(unix)]
    {
        let path = std::env::temp_dir().join("ozone-test-env.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .expect("open ozone test env lock file");
        let exit_code = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        assert_eq!(exit_code, 0, "acquire ozone test env lock");
        EnvLockGuard { file }
    }

    #[cfg(not(unix))]
    {
        EnvLockGuard {}
    }
}

impl Drop for EnvLockGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}
