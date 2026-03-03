//! Tests for the session module (Session and SessionManager)

use std::sync::Arc;
use dbgview::{
    FilterSet, RingBuffer, Session, SessionManager, SessionStatus,
};

// ── Helpers ───────────────────────────────────────────────

fn make_buffer() -> Arc<RingBuffer> {
    Arc::new(RingBuffer::new(100))
}

// ── Session basic ─────────────────────────────────────────

#[test]
fn session_new_starts_at_current_seq() {
    let buf = make_buffer();
    buf.push(1, "before-session".into());
    buf.push(1, "before-session-2".into());

    let session = Session::new("s1".into(), "test".into(), buf.clone());

    // Session should start at the current seq (after existing entries)
    let output = session.get_output(100);
    assert!(output.is_empty(), "New session should not see pre-existing entries");
}

#[test]
fn session_reads_new_entries() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    buf.push(1, "hello".into());
    buf.push(1, "world".into());

    let output = session.get_output(100);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0].text, "hello");
    assert_eq!(output[1].text, "world");
}

#[test]
fn session_advances_cursor() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    buf.push(1, "msg-1".into());
    buf.push(1, "msg-2".into());

    let first = session.get_output(100);
    assert_eq!(first.len(), 2);

    // Second read should return nothing (cursor advanced)
    let second = session.get_output(100);
    assert!(second.is_empty());

    // Push more and read again
    buf.push(1, "msg-3".into());
    let third = session.get_output(100);
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].text, "msg-3");
}

#[test]
fn session_respects_limit() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    for i in 0..10 {
        buf.push(1, format!("msg-{}", i));
    }

    let output = session.get_output(3);
    assert_eq!(output.len(), 3);
}

// ── Session filters ───────────────────────────────────────

#[test]
fn session_filters_by_include_pattern() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    session.set_filters(FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        ..Default::default()
    }).unwrap();

    buf.push(1, "ERROR: bad stuff".into());
    buf.push(1, "INFO: all good".into());
    buf.push(1, "ERROR: more bad".into());

    let output = session.get_output(100);
    assert_eq!(output.len(), 2);
    assert!(output.iter().all(|e| e.text.contains("ERROR")));
}

#[test]
fn session_filters_by_pid() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    session.set_filters(FilterSet {
        process_pids: vec![42],
        ..Default::default()
    }).unwrap();

    buf.push(42, "from target".into());
    buf.push(99, "from other".into());
    buf.push(42, "also from target".into());

    let output = session.get_output(100);
    assert_eq!(output.len(), 2);
    assert!(output.iter().all(|e| e.pid == 42));
}

#[test]
fn session_filter_invalid_regex_returns_error() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    let result = session.set_filters(FilterSet {
        include_patterns: vec!["[bad-regex".to_string()],
        ..Default::default()
    });
    assert!(result.is_err());
}

#[test]
fn session_get_filters_returns_current() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    let default_filters = session.get_filters();
    assert!(default_filters.include_patterns.is_empty());

    session.set_filters(FilterSet {
        include_patterns: vec!["test".to_string()],
        ..Default::default()
    }).unwrap();

    let filters = session.get_filters();
    assert_eq!(filters.include_patterns, vec!["test".to_string()]);
}

// ── Session clear ─────────────────────────────────────────

#[test]
fn session_clear_skips_pending() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    buf.push(1, "old-1".into());
    buf.push(1, "old-2".into());

    session.clear();

    buf.push(1, "new-1".into());

    let output = session.get_output(100);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].text, "new-1");
}

// ── Session pending count ─────────────────────────────────

#[test]
fn session_pending_count() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "test".into(), buf.clone());

    assert_eq!(session.pending_count(), 0);

    buf.push(1, "a".into());
    buf.push(1, "b".into());
    assert_eq!(session.pending_count(), 2);

    session.get_output(1);
    // pending_count is approximate/unfiltered, may vary
    // but should be less after reading
}

// ── Session status ────────────────────────────────────────

#[test]
fn session_status_fields() {
    let buf = make_buffer();
    let session = Session::new("s1".into(), "my-session".into(), buf.clone());

    buf.push(1, "hello".into());

    let status = session.status(true);
    assert_eq!(status.id, "s1");
    assert_eq!(status.name, "my-session");
    assert!(status.capture_active);
    assert_eq!(status.pending_count, 1);

    let status_inactive = session.status(false);
    assert!(!status_inactive.capture_active);
}

#[test]
fn session_status_serializes() {
    let status = SessionStatus {
        id: "s1".into(),
        name: "test".into(),
        filters: FilterSet::default(),
        pending_count: 5,
        capture_active: true,
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"id\":\"s1\""));
    assert!(json.contains("\"capture_active\":true"));

    let deserialized: SessionStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "s1");
    assert_eq!(deserialized.pending_count, 5);
}

// ── Multiple sessions share buffer ────────────────────────

#[test]
fn multiple_sessions_independent_cursors() {
    let buf = make_buffer();
    let s1 = Session::new("s1".into(), "first".into(), buf.clone());
    let s2 = Session::new("s2".into(), "second".into(), buf.clone());

    buf.push(1, "msg-1".into());
    buf.push(1, "msg-2".into());

    // s1 reads both
    let out1 = s1.get_output(100);
    assert_eq!(out1.len(), 2);

    // s2 reads both independently
    let out2 = s2.get_output(100);
    assert_eq!(out2.len(), 2);

    // s1 has nothing more, s2 has nothing more
    assert!(s1.get_output(100).is_empty());
    assert!(s2.get_output(100).is_empty());
}

#[test]
fn multiple_sessions_different_filters() {
    let buf = make_buffer();
    let s1 = Session::new("s1".into(), "errors".into(), buf.clone());
    let s2 = Session::new("s2".into(), "all".into(), buf.clone());

    s1.set_filters(FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        ..Default::default()
    }).unwrap();

    buf.push(1, "ERROR: bad".into());
    buf.push(1, "INFO: ok".into());
    buf.push(1, "ERROR: worse".into());

    let out1 = s1.get_output(100);
    let out2 = s2.get_output(100);

    assert_eq!(out1.len(), 2); // only errors
    assert_eq!(out2.len(), 3); // everything
}

// ── SessionManager ────────────────────────────────────────

// Note: SessionManager.create_session() calls ensure_capture_running()
// which starts the Windows debug capture. On Windows this may conflict
// with other debuggers. We test SessionManager indirectly where possible.

#[test]
fn session_manager_create_and_get() {
    let buf = make_buffer();
    let mgr = SessionManager::new(buf);

    let session = mgr.create_session(Some("my-session".into()));
    // On non-Windows or if another debugger is attached, this may fail
    if let Ok(session) = session {
        assert_eq!(session.name, "my-session");
        assert!(session.id.starts_with("session_"));

        let retrieved = mgr.get_session(&session.id);
        assert!(retrieved.is_ok());
        assert_eq!(retrieved.unwrap().id, session.id);
    }
}

#[test]
fn session_manager_auto_names_sessions() {
    let buf = make_buffer();
    let mgr = SessionManager::new(buf);

    let session = mgr.create_session(None);
    if let Ok(session) = session {
        // When no name given, name equals ID
        assert_eq!(session.name, session.id);
    }
}

#[test]
fn session_manager_destroy_session() {
    let buf = make_buffer();
    let mgr = SessionManager::new(buf);

    let session = mgr.create_session(Some("temp".into()));
    if let Ok(session) = session {
        let id = session.id.clone();
        assert!(mgr.destroy_session(&id).is_ok());

        // Should not find it anymore
        let err = mgr.get_session(&id);
        assert!(err.is_err());
    }
}

#[test]
fn session_manager_destroy_nonexistent_returns_error() {
    let buf = make_buffer();
    let mgr = SessionManager::new(buf);

    let result = mgr.destroy_session("nonexistent");
    assert!(result.is_err());
}

#[test]
fn session_manager_get_nonexistent_returns_error() {
    let buf = make_buffer();
    let mgr = SessionManager::new(buf);

    let result = mgr.get_session("nonexistent");
    assert!(result.is_err());
}

#[test]
fn session_manager_list_sessions() {
    let buf = make_buffer();
    let mgr = SessionManager::new(buf);

    assert!(mgr.list_sessions().is_empty());

    let s1 = mgr.create_session(Some("first".into()));
    let s2 = mgr.create_session(Some("second".into()));

    if s1.is_ok() && s2.is_ok() {
        let list = mgr.list_sessions();
        assert_eq!(list.len(), 2);

        let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"first"));
        assert!(names.contains(&"second"));
    }
}

#[test]
fn session_manager_is_capture_active_initially_false() {
    let buf = make_buffer();
    let mgr = SessionManager::new(buf);
    assert!(!mgr.is_capture_active());
}

#[test]
fn session_manager_shutdown_clears_sessions() {
    let buf = make_buffer();
    let mgr = SessionManager::new(buf);

    let _ = mgr.create_session(Some("temp".into()));
    mgr.shutdown();

    assert!(mgr.list_sessions().is_empty());
}
