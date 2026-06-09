//! Internal utilities.

#[inline]
pub const fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

pub fn human_size(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB"];
    let mut size = bytes as f64;
    let mut i = 0;
    while size >= 1024.0 && i < UNITS.len() - 1 { size /= 1024.0; i += 1; }
    if i == 0 { format!("{bytes} B") } else { format!("{size:.2} {}", UNITS[i]) }
}
