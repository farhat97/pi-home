//! Runs real tmux commands against a throwaway server.

use std::process::Command;

use pihome_rotator::tmux::{Switcher, Tmux};

struct Server(String);

impl Server {
    fn start() -> Self {
        let socket = format!("pihome-test-{}", std::process::id());
        let tmux = |args: &[&str]| {
            let ok = Command::new("tmux")
                .args(["-L", &socket, "-f", "/dev/null"])
                .args(args)
                .status()
                .expect("tmux must be installed")
                .success();
            assert!(ok, "tmux {args:?}");
        };
        tmux(&["new-session", "-d", "-s", "t", "-n", "one", "sleep 60"]);
        tmux(&["new-window", "-t", "t", "-n", "two", "sleep 60"]);
        Self(socket)
    }

    fn current(&self) -> String {
        let out = Command::new("tmux")
            .args(["-L", &self.0, "display-message", "-p", "-t", "=t:", "#W"])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.0, "kill-server"])
            .status();
    }
}

#[test]
fn selects_windows_in_a_real_session() {
    let server = Server::start();
    let mut tmux = Tmux::new("t").with_socket(&server.0);
    assert_eq!(server.current(), "two");
    tmux.select("one").unwrap();
    assert_eq!(server.current(), "one");
    tmux.select("1").unwrap();
    assert_eq!(server.current(), "two");
    // A prefix of a window name must not match it.
    assert!(tmux.select("on").is_err());
    assert_eq!(server.current(), "two");
    let err = tmux.select("missing").unwrap_err().to_string();
    assert!(err.contains("missing"), "{err}");
    assert!(
        Tmux::new("no-such-session")
            .with_socket(&server.0)
            .select("one")
            .is_err()
    );
}
