use std::{
    env,
    net::{IpAddr, Ipv4Addr},
    sync::mpsc,
    thread,
    time::Duration,
};

#[derive(Debug)]
struct PingResult {
    msg: String,
    sort_key: u32,
}

fn parse_ip(s: &str) -> Option<IpAddr> {
    s.parse().ok()
}

fn v4_key(ip: Ipv4Addr) -> u32 {
    u32::from_be_bytes(ip.octets())
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly", target_os = "macos"))]
fn ping_unix_cmd(ip: &str, timeout: Duration) -> bool {
    use std::process::{Command, Stdio};

    // Choose flags per OS
    #[cfg(target_os = "macos")]
    let args = {
        // macOS: -c <count>, -n no DNS, -W <ms> per-reply wait
        let ms = timeout.as_millis().clamp(1, 60_000) as u128;
        ["-n", "-c", "1", "-W", &ms.to_string(), ip]
    };

    #[cfg(not(target_os = "macos"))]
    let args = {
        // Linux/*BSD: -c <count>, -n no DNS, -W <sec> per-reply wait
        let secs = timeout.as_secs().max(1).to_string();
        ["-n", "-c", "1", "-W", &secs, ip]
    };

    let status = Command::new("ping")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    matches!(status.as_ref().map(|s| s.success()), Ok(true))
}

#[cfg(windows)]
fn ping_windows_icmp(ipv4: Ipv4Addr, timeout: Duration) -> bool {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::NetworkManagement::IpHelper::{IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho};

    unsafe {
        let h: HANDLE = IcmpCreateFile();
        if h == 0 {
            return false;
        }

        // Convert IPv4 to network-order u32
        let addr_u32 = u32::from(ipv4).to_be();

        // Small request/response buffers
        let req: [u8; 8] = [0x61; 8];
        let mut reply = [0u8; 64];

        let ok = IcmpSendEcho(
            h,
            addr_u32,
            req.as_ptr() as *const c_void,
            req.len() as u16,
            std::ptr::null_mut(),
            reply.as_mut_ptr() as *mut c_void,
            reply.len() as u32,
            timeout.as_millis().min(u128::from(u32::MAX)) as u32,
        ) > 0;

        IcmpCloseHandle(h);
        ok
    }
}

fn ping_one(ip_str: String, tx: mpsc::Sender<PingResult>) {
    let parsed = match parse_ip(&ip_str) {
        Some(ip) => ip,
        None => {
            let _ = tx.send(PingResult {
                sort_key: 0,
                msg: format!("\x1b[0m{}\x1b[0m is \x1b[1m\x1b[31minvalid\x1b[0m", ip_str),
            });
            return;
        }
    };

    let v4 = match parsed {
        IpAddr::V4(v4) => v4,
        IpAddr::V6(_) => {
            let _ = tx.send(PingResult {
                sort_key: 0,
                msg: format!("\x1b[0m{}\x1b[0m is \x1b[33mIPv6 currently unsupported\x1b[0m", ip_str),
            });
            return;
        }
    };

    let timeout = Duration::from_secs(1);

    #[cfg(windows)]
    let up = ping_windows_icmp(v4, timeout);

    #[cfg(not(windows))]
    let up = ping_unix_cmd(&ip_str, timeout);

    let msg = if up {
        format!("\x1b[1m{}\x1b[0m is \x1b[1m\x1b[32mup\x1b[0m", ip_str)
    } else {
        format!("\x1b[0m{}\x1b[0m is \x1b[1m\x1b[31mdown\x1b[0m", ip_str)
    };

    let _ = tx.send(PingResult {
        sort_key: v4_key(v4),
        msg,
    });
}

fn main() {
    let ips: Vec<String> = env::args().skip(1).collect();
    if ips.is_empty() {
        eprintln!("Usage: ipchk <IP1> <IP2> <IP3> ...");
        return;
    }

    let (tx, rx) = mpsc::channel::<PingResult>();

    let mut handles = Vec::with_capacity(ips.len());
    for ip in ips.into_iter() {
        let txc = tx.clone();
        handles.push(thread::spawn(move || ping_one(ip, txc)));
    }
    drop(tx);

    let mut results = Vec::new();
    for r in rx {
        results.push(r);
    }
    for h in handles {
        let _ = h.join();
    }

    results.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
    for r in results {
        println!("{}", r.msg);
    }
}

