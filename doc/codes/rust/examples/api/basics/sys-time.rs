use core::time::Duration;

use chrono::{TimeZone, Utc};

use autd3_rs::value::SysTime;

fn main() {
    // ANCHOR: construct
    let epoch = SysTime::ZERO;
    let now = SysTime::now().unwrap();
    let at = SysTime::from_utc(Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()).unwrap();
    let raw = SysTime::from_nanos(1_000_000_000);
    let ns: u64 = now.sys_time();
    let utc = now.to_utc();
    // ANCHOR_END: construct

    // ANCHOR: ops
    let future = SysTime::now().unwrap() + Duration::from_millis(100);
    let past = future - Duration::from_millis(50);
    let elapsed: Duration = future - past;
    let is_after: bool = future > past; // true
    // ANCHOR_END: ops

    let _ = (epoch, at, raw, ns, utc, elapsed, is_after);
}
