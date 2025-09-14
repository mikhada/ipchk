use time::{OffsetDateTime, UtcOffset, format_description::parse};

fn main() {
    let now_utc = OffsetDateTime::now_utc().to_offset(UtcOffset::UTC);

    // Custom format: %Y-%m-%dT%H:%M:%SZ
    let fmt = parse("[year]-[month]-[day]T[hour]:[minute]:[second]Z").expect("valid format");
    let iso = now_utc.format(&fmt).expect("format datetime");

    println!("cargo:rustc-env=BUILD_DATE={}", iso);
}
