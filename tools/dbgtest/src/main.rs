//! Debug Output Test Application
//!
//! A configurable test application that produces Windows debug output (OutputDebugString)
//! for testing the Sysinternals MCP server capabilities.
//!
//! Usage:
//!     dbgtest.exe [options]
//!
//! Examples:
//!     # Basic: Send 10 messages with [TEST] tag
//!     dbgtest.exe
//!
//!     # Custom tag and count
//!     dbgtest.exe --tag ERROR --count 50 --interval 100
//!
//!     # Multiple tags for filter testing
//!     dbgtest.exe --multi-tag
//!
//!     # Continuous mode for real-time monitoring
//!     dbgtest.exe --continuous --interval 1000
//!
//!     # Burst mode for stress testing
//!     dbgtest.exe --burst 1000
//!
//!     # Pattern mode for regex testing
//!     dbgtest.exe --pattern --count 3

use clap::{Parser, Subcommand};
use chrono::Local;
use rand::Rng;
use std::io::{self, BufRead, Write};

#[cfg(windows)]
use windows::core::PCSTR;

#[cfg(windows)]
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringA;

/// Debug Output Test Application for MCP Server Testing
#[derive(Parser)]
#[command(name = "dbgtest")]
#[command(about = "Generate Windows debug output (OutputDebugString) for testing")]
#[command(version)]
struct Cli {
    /// Message tag/prefix
    #[arg(short, long, default_value = "TEST")]
    tag: String,

    /// Number of messages to send
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

    /// Mode selection
    #[command(subcommand)]
    mode: Option<Mode>,
}

#[derive(Subcommand)]
enum Mode {
    /// Use multiple random tags for filter testing (INFO, DEBUG, WARN, ERROR, TRACE, VERBOSE)
    MultiTag,
    
    /// Run continuously until interrupted (Ctrl+C)
    Continuous,
    
    /// Send N messages as fast as possible
    Burst {
        /// Number of messages to burst
        #[arg(default_value = "1000")]
        count: u32,
    },
    
    /// Send diverse patterns for regex testing
    Pattern,
    
    /// Interactive mode - type messages to send
    Interactive,
}

/// Send a debug string via Windows OutputDebugStringA
#[cfg(windows)]
fn output_debug_string(message: &str) {
    let msg = format!("{}\0", message);
    unsafe {
        OutputDebugStringA(PCSTR::from_raw(msg.as_ptr()));
    }
}

#[cfg(not(windows))]
fn output_debug_string(message: &str) {
    eprintln!("[SIMULATED DEBUG] {}", message);
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

/// Get current timestamp as formatted string
fn timestamp() -> String {
    Local::now().format("%H:%M:%S%.3f").to_string()
}

/// Basic mode: Send a fixed number of messages with a single tag
fn run_basic_mode(cli: &Cli) {
    println!("Sending {} messages with tag [{}]...", cli.count, cli.tag);
    println!("Interval: {}ms", cli.interval);
    println!();

    for i in 1..=cli.count {
        let message = format!(
            "[{}] [{}] Message {}/{}: {}",
            cli.tag,
            timestamp(),
            i,
            cli.count,
            cli.message
        );

        output_debug_string(&message);
        println!("  Sent: {}", message);

        if i < cli.count {
            std::thread::sleep(std::time::Duration::from_millis(cli.interval));
        }
    }

    println!("\nDone! Sent {} messages.", cli.count);
}

/// Multi-tag mode: Send messages with multiple random tags for filter testing
fn run_multi_tag_mode(cli: &Cli) {
    let tags = ["INFO", "DEBUG", "WARN", "ERROR", "TRACE", "VERBOSE"];
    let levels = [
        ("ERROR", "Critical failure detected!"),
        ("WARN", "Warning: potential issue"),
        ("INFO", "Informational message"),
        ("DEBUG", "Debug details here"),
        ("TRACE", "Trace: entering function"),
        ("VERBOSE", "Verbose: detailed trace data"),
    ];

    println!("Sending {} messages with multiple tags: {:?}", cli.count, tags);
    println!("Use MCP filters to test include/exclude patterns!");
    println!();

    let mut rng = rand::thread_rng();

    for i in 1..=cli.count {
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

        if i < cli.count {
            std::thread::sleep(std::time::Duration::from_millis(cli.interval));
        }
    }

    println!("\nDone! Sent {} messages across {} tags.", cli.count, tags.len());
    println!("\nFilter suggestions:");
    println!("  - Include only errors: set_filters(include_patterns: [\"\\\\[ERROR\\\\]\"])");
    println!("  - Exclude verbose: set_filters(exclude_patterns: [\"\\\\[VERBOSE\\\\]\", \"\\\\[TRACE\\\\]\"])");
}

/// Continuous mode: Run continuously until interrupted
fn run_continuous_mode(cli: &Cli) {
    println!("Continuous mode - sending [{}] messages every {}ms", cli.tag, cli.interval);
    println!("Press Ctrl+C to stop...");
    println!();

    let mut count = 0u64;

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
            cli.tag,
            timestamp(),
            count,
            cli.message
        );

        output_debug_string(&message);
        println!("  Sent #{}: {}", count, message);

        std::thread::sleep(std::time::Duration::from_millis(cli.interval));
    }

    println!("\n\nStopped after {} messages.", count);
}

/// Simple Ctrl+C handler setup
fn ctrlc_handler<F: FnOnce() + Send + 'static>(handler: F) {
    let handler = std::sync::Mutex::new(Some(handler));
    ctrlc::set_handler(move || {
        if let Some(h) = handler.lock().unwrap().take() {
            h();
        }
    }).expect("Error setting Ctrl-C handler");
}

/// Burst mode: Send messages as fast as possible
fn run_burst_mode(burst_count: u32) {
    println!("Burst mode - sending {} messages as fast as possible...", burst_count);

    let start = std::time::Instant::now();

    for i in 1..=burst_count {
        let message = format!("[BURST] #{}: {}", i, generate_random_data(20));
        output_debug_string(&message);
    }

    let elapsed = start.elapsed();
    let rate = burst_count as f64 / elapsed.as_secs_f64();

    println!("Done! Sent {} messages in {:.3}s ({:.0} msg/s)", burst_count, elapsed.as_secs_f64(), rate);
    println!("\nThis tests the ring buffer and high-throughput capture.");
}

/// Pattern mode: Send diverse message patterns for regex testing
fn run_pattern_mode(cli: &Cli) {
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
    println!("Sending {} unique patterns, {} iterations each", patterns.len(), cli.count);
    println!();

    let mut total = 0;

    for iteration in 0..cli.count {
        for (pattern, category) in &patterns {
            let message = format!("{} @{}", pattern, timestamp());
            output_debug_string(&message);
            total += 1;

            if cli.verbose {
                println!("  [{}] {}", category, message);
            }
        }

        if iteration < cli.count - 1 {
            std::thread::sleep(std::time::Duration::from_millis(cli.interval));
        }
    }

    println!("\nDone! Sent {} messages.", total);
    println!("\nFilter suggestions:");
    println!("  - Database queries: set_filters(include_patterns: [\"\\\\[DB:\"])");
    println!("  - HTTP traffic: set_filters(include_patterns: [\"\\\\[HTTP:\"])");
    println!("  - Errors and warnings: set_filters(include_patterns: [\"\\\\[ERROR\\\\]\", \"\\\\[WARN\\\\]\"])");
    println!("  - Exclude performance: set_filters(exclude_patterns: [\"\\\\[PERF\\\\]\"])");
}

/// Interactive mode: Type messages to send
fn run_interactive_mode(cli: &Cli) {
    println!("Interactive mode - type messages to send as debug output");
    println!("Messages will be prefixed with [{}]", cli.tag);
    println!("Type 'quit' or press Ctrl+C to exit");
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("[{}] > ", cli.tag);
        stdout.flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let input = line.trim();
                if input.eq_ignore_ascii_case("quit") {
                    break;
                }
                if !input.is_empty() {
                    let message = format!("[{}] [{}] {}", cli.tag, timestamp(), input);
                    output_debug_string(&message);
                    println!("  Sent: {}", message);
                }
            }
            Err(_) => break,
        }
    }

    println!("\nExiting interactive mode.");
}

fn main() {
    #[cfg(not(windows))]
    {
        eprintln!("Warning: This application is designed for Windows. Debug output will be simulated.");
    }

    let cli = Cli::parse();

    println!("{}", "=".repeat(60));
    println!("Debug Output Test Application");
    println!("{}", "=".repeat(60));
    
    #[cfg(windows)]
    {
        let pid = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };
        println!("PID: {}", pid);
    }
    
    println!();

    match &cli.mode {
        Some(Mode::MultiTag) => run_multi_tag_mode(&cli),
        Some(Mode::Continuous) => run_continuous_mode(&cli),
        Some(Mode::Burst { count }) => run_burst_mode(*count),
        Some(Mode::Pattern) => run_pattern_mode(&cli),
        Some(Mode::Interactive) => run_interactive_mode(&cli),
        None => run_basic_mode(&cli),
    }
}
