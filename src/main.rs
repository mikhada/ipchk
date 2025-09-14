use pico_args::Arguments;
use std::{
    env,
    net::{IpAddr, Ipv4Addr},
    sync::mpsc,
    thread,
    time::Duration,
};

const DEFAULT_TIMEOUT_MS: u64 = 2000;
const DEFAULT_COUNT: u32 = 4;
const DEFAULT_CONCURRENCY: usize = 128;

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

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly",
    target_os = "macos"
))]
fn ping_unix_cmd(ip: &str, timeout: Duration, count: u32) -> bool {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new("ping");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg("-n")
        .arg("-c")
        .arg(count.to_string());

    // Per-reply timeout: macOS uses ms, most others use seconds
    #[cfg(target_os = "macos")]
    {
        let ms = timeout.as_millis().clamp(1, 60_000) as u128;
        cmd.arg("-W").arg(ms.to_string());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let secs = timeout.as_secs().max(1).to_string();
        cmd.arg("-W").arg(secs);
    }

    let status = cmd.arg(ip).status();
    matches!(status.as_ref().map(|s| s.success()), Ok(true))
}

#[cfg(windows)]
fn ping_windows_icmp(ipv4: Ipv4Addr, timeout: Duration, count: u32) -> bool {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho,
    };

    unsafe {
        let h: HANDLE = IcmpCreateFile();
        if h == 0 {
            return false;
        }

        let addr_u32 = u32::from(ipv4).to_be();
        let req: [u8; 8] = [0x61; 8];
        let mut reply = [0u8; 64];

        let mut ok_any = false;
        let tries = count.max(1);
        for _ in 0..tries {
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
            if ok {
                ok_any = true;
                break;
            }
        }

        IcmpCloseHandle(h);
        ok_any
    }
}

fn ping_one(ip_str: String, tx: mpsc::Sender<PingResult>, timeout: Duration, count: u32) {
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
                msg: format!(
                    "\x1b[0m{}\x1b[0m is \x1b[33mIPv6 currently unsupported\x1b[0m",
                    ip_str
                ),
            });
            return;
        }
    };

    #[cfg(windows)]
    let up = ping_windows_icmp(v4, timeout, count);

    #[cfg(not(windows))]
    let up = ping_unix_cmd(&ip_str, timeout, count);

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

/* -------------------- pico-args plumbing -------------------- */

#[derive(Debug)]
struct Args {
    range: Option<(Ipv4Addr, Ipv4Addr)>, // -r/--range start end
    timeout_ms: u64,                     // -t/--timeout (ms)
    count: u32,                          // -n/--count probes per host
    concurrency: usize,                  // -c/--concurrency
    ips: Vec<String>,                    // positional IPs
}

fn usage(program: &str) -> String {
    format!(
        "Usage:
  {p} <IP1> <IP2> ...                       # ping positional addresses
  {p} -r <start_ipv4> <end_ipv4>            # ping inclusive IPv4 range

Options:
  -r, --range            Upper- and lower-limit IPv4 addresses (inclusive)
  -t, --timeout          Per-probe timeout in milliseconds (default: {dto})
  -n, --count            Probes per host; succeed on first reply (default: {dn})
  -c, --concurrency      Max simultaneous hosts in flight (default: {dc})
  -h, --help             Show this help

Examples:
  {p} 192.168.1.1 192.168.1.2 1.1.1.1
  {p} -r 172.16.0.1 172.16.1.254 -t 750 -n 3 -c 256
",
        p = program,
        dto = DEFAULT_TIMEOUT_MS,
        dn = DEFAULT_COUNT,
        dc = DEFAULT_CONCURRENCY
    )
}

fn parse_args() -> Result<Args, String> {
    let mut pargs = Arguments::from_env();
    let program = env::args().next().unwrap_or_else(|| "ipchk".to_string());

    if pargs.contains(["-h", "--help"]) {
        return Err(usage(&program));
    }

    let timeout_ms = pargs
        .opt_value_from_str::<_, u64>(["-t", "--timeout"])
        .map_err(|e| format!("--timeout: {e}"))?
        .unwrap_or(DEFAULT_TIMEOUT_MS);

    let count = pargs
        .opt_value_from_str::<_, u32>(["-n", "--count"])
        .map_err(|e| format!("--count: {e}"))?
        .unwrap_or(DEFAULT_COUNT)
        .max(1);

    let concurrency = pargs
        .opt_value_from_str::<_, usize>(["-c", "--concurrency"])
        .map_err(|e| format!("--concurrency: {e}"))?
        .unwrap_or(DEFAULT_CONCURRENCY)
        .max(1);

    let range_mode = pargs.contains(["-r", "--range"]);
    let free: Vec<std::ffi::OsString> = pargs.finish();

    if range_mode {
        if free.len() != 2 {
            return Err("Usage: ipchk -r <start_ipv4> <end_ipv4>".into());
        }
        let start_str = free[0].to_string_lossy();
        let end_str = free[1].to_string_lossy();

        let start: Ipv4Addr = start_str
            .parse::<Ipv4Addr>()
            .map_err(|_| format!("range: start must be IPv4: {start_str}"))?;
        let end: Ipv4Addr = end_str
            .parse::<Ipv4Addr>()
            .map_err(|_| format!("range: end must be IPv4: {end_str}"))?;

        Ok(Args {
            range: Some((start, end)),
            timeout_ms,
            count,
            concurrency,
            ips: Vec::new(),
        })
    } else {
        let ips: Vec<String> = free
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();

        if ips.is_empty() {
            return Err(usage(&program));
        }

        Ok(Args {
            range: None,
            timeout_ms,
            count,
            concurrency,
            ips,
        })
    }
}

/* -------------------- range iterator + main -------------------- */

struct IpRange {
    cur: u32,
    end: u32,
} // inclusive
impl IpRange {
    fn new(a: Ipv4Addr, b: Ipv4Addr) -> Self {
        let mut lo = u32::from_be_bytes(a.octets());
        let mut hi = u32::from_be_bytes(b.octets());
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        IpRange { cur: lo, end: hi }
    }
}
impl Iterator for IpRange {
    type Item = Ipv4Addr;
    fn next(&mut self) -> Option<Self::Item> {
        if self.cur > self.end {
            return None;
        }
        let out = Ipv4Addr::from(self.cur.to_be_bytes());
        self.cur = self.cur.wrapping_add(1);
        Some(out)
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(if msg.starts_with("Usage:") { 0 } else { 2 });
        }
    };

    let timeout = Duration::from_millis(args.timeout_ms);
    let count = args.count;

    let (tx, rx) = mpsc::channel::<PingResult>();

    // Helper to spawn a bounded batch to avoid thousands of threads
    let spawn_batch = |batch: Vec<String>, tx: &mpsc::Sender<PingResult>| {
        let mut handles = Vec::with_capacity(batch.len());
        for ip in batch {
            let txc = tx.clone();
            let tmo = timeout;
            let cnt = count;
            handles.push(thread::spawn(move || ping_one(ip, txc, tmo, cnt)));
        }
        for h in handles {
            let _ = h.join();
        }
    };

    if let Some((start, end)) = args.range {
        let mut iter = IpRange::new(start, end);
        loop {
            let mut batch = Vec::with_capacity(args.concurrency);
            for _ in 0..args.concurrency {
                if let Some(ip) = iter.next() {
                    batch.push(ip.to_string());
                } else {
                    break;
                }
            }
            if batch.is_empty() {
                break;
            }
            spawn_batch(batch, &tx);
        }
    } else {
        let mut it = args.ips.into_iter();
        loop {
            let mut batch = Vec::with_capacity(args.concurrency);
            for _ in 0..args.concurrency {
                if let Some(ip) = it.next() {
                    batch.push(ip);
                } else {
                    break;
                }
            }
            if batch.is_empty() {
                break;
            }
            spawn_batch(batch, &tx);
        }
    }
    drop(tx);

    let mut results = Vec::new();
    for r in rx {
        results.push(r);
    }
    results.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
    for r in results {
        println!("{}", r.msg);
    }
}
