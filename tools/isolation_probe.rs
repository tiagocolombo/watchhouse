//! Trusted, dependency-free Linux qualification payload. Compile statically; never runs candidate code.
use std::{
    env, fs,
    io::{self, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    process::{Command, Stdio},
    thread,
    time::Duration,
};
fn main() {
    let args = env::args().collect::<Vec<_>>();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    if mode == "pulse" {
        let mut n = 0u64;
        loop {
            n += 1;
            let _ = fs::write("/work/pulse", n.to_string());
            thread::sleep(Duration::from_millis(20));
        }
    }
    let nonce = args.get(2).expect("nonce");
    assert!(nonce.len() == 36 && nonce.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'));
    if mode == "descendants" {
        let mut child = Command::new("/probe")
            .arg("pulse")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn descendant");
        for _ in 0..100 {
            if fs::read_to_string("/work/pulse").is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        println!("{{\"schema_version\":1,\"nonce\":\"{nonce}\",\"started\":true}}");
        io::stdout().flush().unwrap();
        let _ = child.wait();
        return;
    }
    assert_eq!(mode, "boundaries");
    let forbidden = &args[3];
    let port = args[4].parse::<u16>().unwrap();
    let input = fs::read_to_string("/input/value").ok().as_deref() == Some(nonce);
    let base_ro = fs::write("/input/value", "tamper").is_err();
    let policy_ro = fs::write("/policy", "tamper").is_err();
    let outside_hidden = fs::read(forbidden).is_err();
    let environment_clean =
        env::var_os("WATCHHOUSE_PROBE_SECRET").is_none() && env::var_os("OPENAI_API_KEY").is_none();
    let work_rw = fs::write("/work/result", nonce).is_ok()
        && fs::read_to_string("/work/result").ok().as_deref() == Some(nonce);
    let network_denied = TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        Duration::from_millis(300),
    )
    .is_err();
    let netns = fs::read_link("/proc/self/ns/net")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let pidns = fs::read_link("/proc/self/ns/pid")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let limits = fs::read_to_string("/proc/self/limits").unwrap_or_default();
    let limit = |prefix: &str| -> u64 {
        limits
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .and_then(|r| r.split_whitespace().next())
            .and_then(|v| v.parse().ok())
            .unwrap_or(u64::MAX)
    };
    let cpu = limit("Max cpu time");
    let memory = limit("Max address space");
    let file = limit("Max file size");
    let descriptors = limit("Max open files");
    let core = limit("Max core file size");
    println!(
        "{{\"schema_version\":1,\"nonce\":\"{nonce}\",\"input_read\":{input},\"base_read_only\":{base_ro},\"policy_read_only\":{policy_ro},\"outside_hidden\":{outside_hidden},\"environment_clean\":{environment_clean},\"work_read_write\":{work_rw},\"network_denied\":{network_denied},\"net_namespace\":\"{netns}\",\"pid_namespace\":\"{pidns}\",\"cpu_seconds\":{cpu},\"memory_bytes\":{memory},\"file_bytes\":{file},\"descriptors\":{descriptors},\"core_bytes\":{core}}}"
    );
}
