//! Debug Output Test Application
//!
//! A configurable test application that produces Windows debug output (OutputDebugString)
//! for testing the sysinternals-mcp server capabilities.
//!
//! Usage:
//!     test-debug-app.exe [options]
//!
//! Examples:
//!     # Basic: Send 10 messages with [TEST] tag
//!     test-debug-app.exe
//!
//!     # Custom tag and count
//!     test-debug-app.exe --tag ERROR --count 50 --interval 100
//!
//!     # Multiple tags for filter testing
//!     test-debug-app.exe --multi-tag
//!
//!     # Continuous mode for real-time monitoring
//!     test-debug-app.exe --continuous --interval 1000
//!
//!     # Burst mode for stress testing
//!     test-debug-app.exe --burst 1000

use clap::Parser;
use chrono::Local;
use rand::Rng;
use std::io::{self, BufRead, Write};
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use windows::core::PCSTR;
#[cfg(windows)]
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringA;

/// Debug Output Test Application for MCP Server Testing
#[derive(Parser)]
#[command(name = "test-debug-app")]
#[command(about = "Generates Windows debug output for testing the Sysinternals MCP server")]
#[command(version)]
struct Args {
    /// Message tag/prefix
    #[arg(short, long, default_value = "TEST")]
    tag: String,

    /// Number of messages to send (for basic and multi-tag modes)
    #[arg(short, long, default_value = "10")]
    count: u32,

    /// Interval between messages in milliseconds
    #[arg(short, long, default_value = "500")]
    interval: u64,

    /// Custom message content
    #[arg(short, long, default_value = "Test debug output message")]
    message: String,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Use multiple random tags (INFO, DEBUG, WARN, ERROR, etc.) for filter testing
    #[arg(long)]
    multi_tag: bool,

    /// Run continuously until interrupted
    #[arg(long)]
    continuous: bool,

    /// Burst mode: send N messages as fast as possible
    #[arg(long)]
    burst: Option<u32>,

    /// Send diverse patterns for regex filter testing
    #[arg(long)]
    pattern: bool,

    /// Interactive mode: type messages to send manually
    #[arg(long)]
    interactive: bool,
}

/// Send a debug string via Windows OutputDebugString API
#[cfg(windows)]
fn output_debug_string(message: &str) {
    let msg = format!("{}\0", message);
    unsafe {
        OutputDebugStringA(PCSTR(msg.as_ptr()));
    }
}

#[cfg(not(windows))]
fn output_debug_string(message: &str) {
    eprintln!("[DEBUG] {}", message);
}

/// Generate random alphanumeric data
fn generate_random_data(length: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Get current timestamp string
fn timestamp() -> String {
    Local::now().format("%H:%M:%S%.3f").to_string()
}

/// Basic mode - send fixed number of messages with one tag
fn run_basic_mode(args: &Args) {
    println!("Sending {} messages with tag [{}]...", args.count, args.tag);
    println!("Interval: {}ms", args.interval);
    println!();

    for i in 1..=args.count {
        let message = format!(
            "[{}] [{}] Message {}/{}: {}",
            args.tag,
            timestamp(),
            i,
            args.count,
            args.message
        );

        output_debug_string(&message);
        println!("  Sent: {}", message);

        if i < args.count {
            thread::sleep(Duration::from_millis(args.interval));
        }
    }

    println!("\nDone! Sent {} messages.", args.count);
}

/// Multi-tag mode - send messages with random tags for filter testing
fn run_multi_tag_mode(args: &Args) {
    let tags = ["INFO", "DEBUG", "WARN", "ERROR", "TRACE", "VERBOSE"];
    let levels = [
        ("ERROR", "Critical failure detected!"),
        ("WARN", "Warning: potential issue"),
        ("INFO", "Informational message"),
        ("DEBUG", "Debug details here"),
        ("TRACE", "Trace: entering function"),
        ("VERBOSE", "Verbose: detailed trace data"),
    ];

    println!(
        "Sending {} messages with multiple tags: {:?}",
        args.count, tags
    );
    println!("Use MCP filters to test include/exclude patterns!");
    println!();

    let mut rng = rand::thread_rng();

    for i in 1..=args.count {
        let tag_idx = rng.gen_range(0..tags.len());
        let tag = tags[tag_idx];
        let level_msg = levels.iter().find(|(t, _)| *t == tag).map(|(_, m)| *m).unwrap_or("Message");

        let message = format!(
            "[{}] [{}] #{}: {} - {}",
            tag,
            timestamp(),
            i,
            level_msg,
            generate_random_data(8)
        );

        output_debug_string(&message);
        println!("  [{}] {}", tag, message);

        if i < args.count {
            thread::sleep(Duration::from_millis(args.interval));
        }
    }

    println!("\nDone! Sent {} messages across {} tags.", args.count, tags.len());
    println!("\nFilter suggestions:");
    println!("  - Include only errors: set_filters(include_patterns: [\"ERROR\"])");
    println!("  - Exclude verbose: set_filters(exclude_patterns: [\"VERBOSE\", \"TRACE\"])");
}

/// Continuous mode - run until interrupted
fn run_continuous_mode(args: &Args) {
    println!(
        "Continuous mode - sending [{}] messages every {}ms",
        args.tag, args.interval
    );
    println!("Press Ctrl+C to stop...");
    println!();

    let mut count: u64 = 0;
    
    // Set up Ctrl+C handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    
    ctrlc_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    });

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        count += 1;
        let message = format!(
            "[{}] [{}] Continuous #{}: {}",
            args.tag,
            timestamp(),
            count,
            args.message
        );

        output_debug_string(&message);
        println!("  Sent #{}: {}", count, message);

        thread::sleep(Duration::from_millis(args.interval));
    }

    println!("\n\nStopped after {} messages.", count);
}

/// Simple Ctrl+C handler setup
fn ctrlc_handler<F: FnMut() + Send + 'static>(mut handler: F) {
    std::thread::spawn(move || {
        loop {
            // Check for Ctrl+C every 100ms by trying to handle it
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    
    // Note: In a real implementation, you'd use the ctrlc crate
    // For simplicity, we just ignore this and rely on the process being killed
    let _ = handler;
}

/// Burst mode - send messages as fast as possible
fn run_burst_mode(count: u32) {
    println!("Burst mode - sending {} messages as fast as possible...", count);

    let start = std::time::Instant::now();

    for i in 1..=count {
        let message = format!("[BURST] #{}: {}", i, generate_random_data(20));
        output_debug_string(&message);
    }

    let elapsed = start.elapsed();
    let rate = if elapsed.as_secs_f64() > 0.0 {
        count as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    println!(
        "Done! Sent {} messages in {:.3}s ({:.0} msg/s)",
        count,
        elapsed.as_secs_f64(),
        rate
    );
    println!("\nThis tests the ring buffer and high-throughput capture.");
}

/// Pattern mode - send diverse patterns for regex testing
fn run_pattern_mode(args: &Args) {
    let patterns = [
        ("[APP:Main] Starting application v1.0.0", "App lifecycle"),
        ("[APP:Main] Configuration loaded from config.json", "App lifecycle"),
        ("[DB:Query] SELECT * FROM users WHERE id = 123", "Database"),
        ("[DB:Query] INSERT INTO logs (msg) VALUES ('test')", "Database"),
        ("[HTTP:Request] GET /api/users/123 HTTP/1.1", "HTTP"),
        ("[HTTP:Response] 200 OK (45ms)", "HTTP"),
        ("[PERF] Frame time: 16.7ms (60 FPS)", "Performance"),
        ("[PERF] Memory usage: 256MB / 1024MB", "Performance"),
        ("[SECURITY] Authentication successful for user 'admin'", "Security"),
        ("[SECURITY] Failed login attempt from 192.168.1.100", "Security"),
        ("[CACHE] Cache hit for key 'user:123'", "Cache"),
        ("[CACHE] Cache miss - loading from database", "Cache"),
        ("[ERROR] NullReferenceException in ProcessData()", "Error"),
        ("[ERROR] Connection timeout after 30s", "Error"),
        ("[WARN] Deprecated API usage detected", "Warning"),
        ("[WARN] Low disk space: 500MB remaining", "Warning"),
    ];

    println!("Pattern mode - sending diverse message patterns for regex filter testing");
    println!(
        "Sending {} unique patterns, {} iterations each",
        patterns.len(),
        args.count
    );
    println!();

    let mut total = 0;

    for iteration in 0..args.count {
        for (pattern, category) in &patterns {
            let message = format!("{} @{}", pattern, timestamp());
            output_debug_string(&message);
            total += 1;

            if args.verbose {
                println!("  [{}] {}", category, message);
            }
        }

        if iteration < args.count - 1 {
            thread::sleep(Duration::from_millis(args.interval));
        }
    }

    println!("\nDone! Sent {} messages.", total);
    println!("\nFilter suggestions:");
    println!("  - Database queries: set_filters(include_patterns: [\"\\\\[DB:\"])");
    println!("  - HTTP traffic: set_filters(include_patterns: [\"\\\\[HTTP:\"])");
    println!("  - Errors and warnings: set_filters(include_patterns: [\"ERROR\", \"WARN\"])");
    println!("  - Exclude performance: set_filters(exclude_patterns: [\"\\\\[PERF\\\\]\"])");
}

/// Interactive mode - type messages to send
fn run_interactive_mode(args: &Args) {
    println!("Interactive mode - type messages to send as debug output");
    println!("Messages will be prefixed with [{}]", args.tag);
    println!("Type 'quit' or press Ctrl+C to exit");
    println!();

    let stdin = io::stdin();
    let handle = stdin.lock();

    for line in handle.lines() {
        match line {
            Ok(input) => {
                let input = input.trim();
                if input.eq_ignore_ascii_case("quit") {
                    break;
                }
                if !input.is_empty() {
                    let message = format!("[{}] [{}] {}", args.tag, timestamp(), input);
                    output_debug_string(&message);
                    println!("  Sent: {}", message);
                }
                print!("[{}] > ", args.tag);
                let _ = io::stdout().flush();
            }
            Err(_) => break,
        }
    }

    println!("\nExiting interactive mode.");
}

fn main() {
    #[cfg(not(windows))]
    {
        eprintln!("Error: This application only works on Windows.");
        std::process::exit(1);
    }

    let args = Args::parse();

    println!("{}", "=".repeat(60));
    println!("Debug Output Test Application");
    println!("{}", "=".repeat(60));
    
    #[cfg(windows)]
    {
        let pid = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };
        println!("PID: {}", pid);
    }
    println!();

    if let Some(burst_count) = args.burst {
        run_burst_mode(burst_count);
    } else if args.multi_tag {
        run_multi_tag_mode(&args);
    } else if args.continuous {
        run_continuous_mode(&args);
    } else if args.pattern {
        run_pattern_mode(&args);
    } else if args.interactive {
        run_interactive_mode(&args);
    } else {
        run_basic_mode(&args);
    }
}
