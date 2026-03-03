//! Integration tests: end-to-end flows across multiple modules

use std::sync::Arc;
use dbgview::{
    FilterSet, RingBuffer, Session, DbgViewError,
};

// ── End-to-end: buffer → session → filtered output ────────

#[test]
fn full_flow_capture_filter_read() {
    let buf = Arc::new(RingBuffer::new(1000));
    let session = Session::new("s1".into(), "integration".into(), buf.clone());

    // Simulate captured output from multiple processes
    buf.push(100, "[ERROR] Connection refused".into());
    buf.push(200, "[DEBUG] Checking cache".into());
    buf.push(100, "[WARN] Retrying in 5s".into());
    buf.push(300, "[INFO] Server started".into());
    buf.push(100, "[ERROR] Timeout exceeded".into());
    buf.push(200, "[TRACE] Entering function foo".into());

    // Filter to only PID 100 errors
    session.set_filters(FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        process_pids: vec![100],
        ..Default::default()
    }).unwrap();

    let output = session.get_output(100);
    assert_eq!(output.len(), 2);
    assert!(output[0].text.contains("Connection refused"));
    assert!(output[1].text.contains("Timeout exceeded"));
    assert!(output.iter().all(|e| e.pid == 100));
}

#[test]
fn full_flow_multiple_sessions_different_views() {
    let buf = Arc::new(RingBuffer::new(1000));

    // Create sessions before pushing data
    let error_session = Session::new("errors".into(), "errors-only".into(), buf.clone());
    let all_session = Session::new("all".into(), "everything".into(), buf.clone());
    let pid_session = Session::new("pid".into(), "pid-42".into(), buf.clone());

    error_session.set_filters(FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        ..Default::default()
    }).unwrap();

    pid_session.set_filters(FilterSet {
        process_pids: vec![42],
        ..Default::default()
    }).unwrap();

    // Simulate mixed debug output
    buf.push(42, "[ERROR] disk full".into());
    buf.push(99, "[INFO] processing".into());
    buf.push(42, "[INFO] retrying".into());
    buf.push(99, "[ERROR] network down".into());

    let errors = error_session.get_output(100);
    let all = all_session.get_output(100);
    let pid42 = pid_session.get_output(100);

    assert_eq!(errors.len(), 2);  // both ERROR entries
    assert_eq!(all.len(), 4);     // everything
    assert_eq!(pid42.len(), 2);   // only pid 42
}

#[test]
fn incremental_reads_across_sessions() {
    let buf = Arc::new(RingBuffer::new(100));
    let s1 = Session::new("s1".into(), "s1".into(), buf.clone());
    let s2 = Session::new("s2".into(), "s2".into(), buf.clone());

    // Push batch 1
    buf.push(1, "batch1-a".into());
    buf.push(1, "batch1-b".into());

    // s1 reads batch 1
    assert_eq!(s1.get_output(100).len(), 2);

    // Push batch 2
    buf.push(1, "batch2-a".into());

    // s1 only gets new entries, s2 gets everything
    let s1_out = s1.get_output(100);
    let s2_out = s2.get_output(100);

    assert_eq!(s1_out.len(), 1);
    assert_eq!(s1_out[0].text, "batch2-a");
    assert_eq!(s2_out.len(), 3);
}

#[test]
fn filter_change_mid_session() {
    let buf = Arc::new(RingBuffer::new(100));
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    buf.push(1, "ERROR: first".into());
    buf.push(1, "INFO: second".into());

    // Read with no filters
    let out = session.get_output(100);
    assert_eq!(out.len(), 2);

    // Push more, set filter, read again
    buf.push(1, "ERROR: third".into());
    buf.push(1, "INFO: fourth".into());

    session.set_filters(FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        ..Default::default()
    }).unwrap();

    let out = session.get_output(100);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "ERROR: third");
}

#[test]
fn clear_then_read_only_new() {
    let buf = Arc::new(RingBuffer::new(100));
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    buf.push(1, "old-1".into());
    buf.push(1, "old-2".into());

    session.clear();

    buf.push(1, "new-1".into());
    buf.push(1, "new-2".into());

    let out = session.get_output(100);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].text, "new-1");
    assert_eq!(out[1].text, "new-2");
}

// ── Buffer overflow while session is reading ──────────────

#[test]
fn session_handles_buffer_wrap() {
    let buf = Arc::new(RingBuffer::new(5));
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    // Push 3, read them
    for i in 0..3 {
        buf.push(1, format!("early-{}", i));
    }
    let out = session.get_output(100);
    assert_eq!(out.len(), 3);

    // Push 10 more (overflows the 5-entry buffer)
    for i in 0..10 {
        buf.push(1, format!("late-{}", i));
    }

    // Session should still get entries (from what's available)
    let out = session.get_output(100);
    assert!(!out.is_empty());
    // Should be the most recent entries
    assert!(out.last().unwrap().text.starts_with("late-"));
}

// ── Concurrent session access ─────────────────────────────

#[test]
fn concurrent_push_and_session_read() {
    let buf = Arc::new(RingBuffer::new(1000));
    let session = Arc::new(Session::new("s1".into(), "test".into(), buf.clone()));

    let writer_buf = buf.clone();
    let writer = std::thread::spawn(move || {
        for i in 0..100 {
            writer_buf.push(1, format!("msg-{}", i));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    let reader_session = session.clone();
    let reader = std::thread::spawn(move || {
        let mut total = 0;
        for _ in 0..200 {
            total += reader_session.get_output(100).len();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        total
    });

    writer.join().unwrap();
    let total_read = reader.join().unwrap();
    assert!(total_read > 0, "Should have read some entries concurrently");
}

// ── Error types ───────────────────────────────────────────

#[test]
fn error_display_messages() {
    let e = DbgViewError::SessionNotFound("xyz".into());
    assert!(e.to_string().contains("xyz"));

    let e = DbgViewError::CaptureNotRunning;
    assert!(e.to_string().contains("not running"));

    let e = DbgViewError::CaptureAlreadyRunning;
    assert!(e.to_string().contains("already running"));

    let e = DbgViewError::PlatformNotSupported;
    assert!(e.to_string().contains("not supported"));

    let e = DbgViewError::DebuggerAlreadyAttached;
    assert!(e.to_string().contains("already attached"));

    let e = DbgViewError::Internal("oops".into());
    assert!(e.to_string().contains("oops"));
}

// ── DebugEntry edge cases ─────────────────────────────────

#[test]
fn empty_message_text() {
    let buf = Arc::new(RingBuffer::new(10));
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    buf.push(1, "".into());

    let out = session.get_output(100);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "");
}

#[test]
fn very_long_message_text() {
    let buf = Arc::new(RingBuffer::new(10));
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    let long_msg = "x".repeat(10_000);
    buf.push(1, long_msg.clone());

    let out = session.get_output(100);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, long_msg);
}

#[test]
fn unicode_in_messages() {
    let buf = Arc::new(RingBuffer::new(10));
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    buf.push(1, "日本語テスト 🎉".into());
    buf.push(1, "Ñoño señor".into());

    let out = session.get_output(100);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].text, "日本語テスト 🎉");
    assert_eq!(out[1].text, "Ñoño señor");
}
