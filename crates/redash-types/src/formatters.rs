/// Formats a raw byte count into human-readable units (B, KB, MB, GB, TB).
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Formats a transfer rate (bytes per second) into human-readable throughput.
pub fn format_speed(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

/// Formats a transfer rate (bytes per second) into human-readable throughput (alias for format_speed).
pub fn format_bytes_rate(bytes_per_sec: u64) -> String {
    format_speed(bytes_per_sec)
}

/// Formats a floating-point percentage (0.0 to 100.0) with 1 decimal place.
pub fn format_percent(pct: f32) -> String {
    format!("{:.1}%", pct.clamp(0.0, 100.0))
}
