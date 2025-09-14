# ipchk
#### A lightweight, parallel ICMP up/down detector written in Rust.
Cross-platform: works on Linux, BSD, macOS, and Windows.
Runs without elevated privileges by deferring to the system `ping` command on Unix-like OSes, and using the Windows ICMP API on Windows.

---

## Features
* Parallel probing of multiple hosts with configurable concurrency*
* IPv4 range support (`-r start end`) without relying on shell expansion
* Configurable timeout (`-t`) and probe count (`-n`)
* Optional plain ASCII output (`-a` / `--ascii` / `--raw`) for piping
* Clean, colourized terminal output by default
* Cross-platform:
  * Unix: uses the native `ping` command
  * Windows: uses the `IcmpSendEcho` API
* Lightweight, small, stripped binary with LTO

**Note:* If you notice unreliable results with larger ranges, try lower concurrency settings. This is not a bug, but could be a limitation of the local ICMP infrastructure.

---

## Installation
### From source
```sh
git clone https://github.com/mikhada/ipchk
cd ipchk
cargo build --release
```

The compiled binary will be at:
```
target/release/ipchk
```

### Requirements

* Rust 1.70+, 2024 edition recommended
* A working `ping` executable in `$PATH` (Linux/macOS/BSD)

---

## Usage

```sh
ipchk [OPTIONS] [IP...]
ipchk -r <START> <END> [OPTIONS]
```

### Options

| Flag                 | Description                                           |
| -------------------- | ----------------------------------------------------- |
| `-r, --range`        | Inclusive IPv4 range (requires `<START>` and `<END>`) |
| `-a, --ascii, --raw` | Force plain ASCII output (disables colour codes)      |
| `-t, --timeout`      | Per-probe timeout in milliseconds (default: `2000`)   |
| `-n, --count`        | Number of probe attempts per host (default: `4`)      |
| `-c, --concurrency`  | Max simultaneous probes in flight (default: `128`)    |
| `-h, --help`         | Show help message and exit                            |
| `--version`          | Show version information and exit                     |

### Examples

**Ping a few individual hosts:**

```sh
ipchk 192.168.1.1 192.168.1.2 1.1.1.1
```

**Ping a /24 range with 64 concurrent threads:**

```sh
ipchk -r 10.0.0.1 10.0.0.254 -c 64
```

**Ping a /23 with longer timeouts and more probes:**

```sh
ipchk -r 172.16.0.1 172.16.1.254 -t 3000 -n 5
```

**Force plain ASCII output for piping:**

```sh
ipchk -r 10.0.0.1 10.0.0.254 --ascii | grep up
```

---

## License
Apache-2.0 License — see [LICENSE](LICENSE) for details.
