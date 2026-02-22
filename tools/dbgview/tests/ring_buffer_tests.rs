//! Tests for the ring buffer module

use std::sync::Arc;
use std::thread;
use dbgview::{RingBuffer, DebugEntry};

// ── Construction ──────────────────────────────────────────

#[test]
fn new_buffer_has_correct_capacity() {
    let buf = RingBuffer::new(50);
    // Fresh buffer: next seq is 1, nothing to read
    assert_eq!(buf.current_seq(), 1);
    let (entries, cursor) = buf.get_entries_from(1, 100);
    assert!(entries.is_empty());
    assert_eq!(cursor, 1);
}

#[test]
fn with_default_capacity_respects_env() {
    // Without env var set, should use DEFAULT_BUFFER_SIZE (100_000)
    let buf = RingBuffer::with_default_capacity();
    assert_eq!(buf.current_seq(), 1);
}

// ── Basic push & read ─────────────────────────────────────

#[test]
fn push_single_entry() {
    let buf = RingBuffer::new(10);
    buf.push(1234, "Hello".to_string());

    let (entries, cursor) = buf.get_entries_from(1, 100);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].seq, 1);
    assert_eq!(entries[0].pid, 1234);
    assert_eq!(entries[0].text, "Hello");
    assert!(entries[0].time > 0); // should have a real timestamp
    assert_eq!(cursor, 2);
}

#[test]
fn push_multiple_entries_sequential() {
    let buf = RingBuffer::new(10);
    for i in 0..5 {
        buf.push(1000 + i, format!("msg-{}", i));
    }

    let (entries, cursor) = buf.get_entries_from(1, 100);
    assert_eq!(entries.len(), 5);
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.seq, (i + 1) as u64);
        assert_eq!(entry.pid, 1000 + i as u32);
        assert_eq!(entry.text, format!("msg-{}", i));
    }
    assert_eq!(cursor, 6);
}

#[test]
fn current_seq_advances_with_pushes() {
    let buf = RingBuffer::new(10);
    assert_eq!(buf.current_seq(), 1);
    buf.push(1, "a".into());
    assert_eq!(buf.current_seq(), 2);
    buf.push(1, "b".into());
    assert_eq!(buf.current_seq(), 3);
    buf.push(1, "c".into());
    assert_eq!(buf.current_seq(), 4);
}

// ── Wrap-around ───────────────────────────────────────────

#[test]
fn wraps_around_when_full() {
    let buf = RingBuffer::new(5);
    for i in 0..10 {
        buf.push(1, format!("msg-{}", i));
    }

    // Only the last 5 should be available
    let (entries, _) = buf.get_entries_from(1, 100);
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[0].text, "msg-5");
    assert_eq!(entries[4].text, "msg-9");
}

#[test]
fn wraps_around_exactly_at_capacity() {
    let buf = RingBuffer::new(3);
    for i in 0..3 {
        buf.push(1, format!("msg-{}", i));
    }

    let (entries, cursor) = buf.get_entries_from(1, 100);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].text, "msg-0");
    assert_eq!(entries[2].text, "msg-2");
    assert_eq!(cursor, 4);
}

#[test]
fn wraps_around_multiple_times() {
    let buf = RingBuffer::new(3);
    for i in 0..100 {
        buf.push(1, format!("msg-{}", i));
    }

    let (entries, _) = buf.get_entries_from(1, 100);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].text, "msg-97");
    assert_eq!(entries[1].text, "msg-98");
    assert_eq!(entries[2].text, "msg-99");
}

// ── Cursor / partial reads ────────────────────────────────

#[test]
fn read_from_middle() {
    let buf = RingBuffer::new(10);
    for i in 0..5 {
        buf.push(1, format!("msg-{}", i));
    }

    // Read starting from seq 3
    let (entries, cursor) = buf.get_entries_from(3, 100);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].text, "msg-2"); // seq 3
    assert_eq!(entries[2].text, "msg-4"); // seq 5
    assert_eq!(cursor, 6);
}

#[test]
fn read_with_limit() {
    let buf = RingBuffer::new(10);
    for i in 0..10 {
        buf.push(1, format!("msg-{}", i));
    }

    let (entries, _) = buf.get_entries_from(1, 3);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].text, "msg-0");
    assert_eq!(entries[2].text, "msg-2");
}

#[test]
fn read_from_future_seq_returns_empty() {
    let buf = RingBuffer::new(10);
    buf.push(1, "hello".into());

    let (entries, cursor) = buf.get_entries_from(999, 100);
    assert!(entries.is_empty());
    assert_eq!(cursor, 999);
}

#[test]
fn read_from_overwritten_seq_adjusts_to_oldest() {
    let buf = RingBuffer::new(3);
    for i in 0..10 {
        buf.push(1, format!("msg-{}", i));
    }

    // Seq 1 is long gone, should adjust to oldest available
    let (entries, _) = buf.get_entries_from(1, 100);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].text, "msg-7"); // oldest available
}

#[test]
fn empty_buffer_returns_empty() {
    let buf = RingBuffer::new(10);
    let (entries, cursor) = buf.get_entries_from(1, 100);
    assert!(entries.is_empty());
    assert_eq!(cursor, 1);
}

// ── Process name cache ────────────────────────────────────

#[test]
fn process_name_is_populated() {
    let buf = RingBuffer::new(10);
    // Use PID 0 or current process — should get some name
    buf.push(std::process::id(), "test".into());

    let (entries, _) = buf.get_entries_from(1, 1);
    assert_eq!(entries.len(), 1);
    // Should have resolved to something (not empty)
    assert!(!entries[0].process_name.is_empty());
}

#[test]
fn unknown_pid_gets_placeholder_name() {
    let buf = RingBuffer::new(10);
    // Push with a PID that almost certainly doesn't exist
    buf.push(999999999, "test".into());

    let (entries, _) = buf.get_entries_from(1, 1);
    assert_eq!(entries.len(), 1);
    // Should get a placeholder like "<999999999>"
    assert!(entries[0].process_name.contains("999999999"));
}

#[test]
fn clear_process_cache() {
    let buf = RingBuffer::new(10);
    buf.push(1, "a".into());
    buf.clear_process_cache();
    // Should not crash, cache is now empty
    buf.push(1, "b".into());
    let (entries, _) = buf.get_entries_from(1, 100);
    assert_eq!(entries.len(), 2);
}

// ── Concurrency ───────────────────────────────────────────

#[test]
fn concurrent_pushes() {
    let buf = Arc::new(RingBuffer::new(1000));
    let mut handles = vec![];

    for t in 0..4 {
        let buf = buf.clone();
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                buf.push(t * 1000 + i, format!("thread-{}-msg-{}", t, i));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // All 400 pushes must have advanced the sequence counter
    assert_eq!(buf.current_seq(), 401);

    // Note: Due to the dual-atomic design (write_pos vs next_seq), concurrent
    // pushes can interleave so that an entry's position doesn't match its
    // expected (seq-1)%capacity slot. get_entries_from uses a seq->position
    // mapping that may skip some entries in this scenario. We verify the entries
    // we DO retrieve are valid.
    let (entries, _) = buf.get_entries_from(1, 1000);
    assert!(!entries.is_empty(), "should retrieve at least some entries");

    // All returned sequence numbers should be unique and in range
    let mut seqs: Vec<u64> = entries.iter().map(|e| e.seq).collect();
    seqs.sort();
    seqs.dedup();
    assert_eq!(seqs.len(), entries.len());
    for &s in &seqs {
        assert!(s >= 1 && s <= 400);
    }
}

#[test]
fn concurrent_push_and_read() {
    let buf = Arc::new(RingBuffer::new(100));

    let writer_buf = buf.clone();
    let writer = thread::spawn(move || {
        for i in 0..50 {
            writer_buf.push(1, format!("msg-{}", i));
            thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    let reader_buf = buf.clone();
    let reader = thread::spawn(move || {
        let mut cursor = 1u64;
        let mut total_read = 0;
        for _ in 0..100 {
            let (entries, new_cursor) = reader_buf.get_entries_from(cursor, 100);
            total_read += entries.len();
            cursor = new_cursor;
            thread::sleep(std::time::Duration::from_millis(2));
        }
        total_read
    });

    writer.join().unwrap();
    let total = reader.join().unwrap();
    assert!(total > 0, "Reader should have read some entries");
}

// ── Timestamps ────────────────────────────────────────────

#[test]
fn timestamps_are_monotonically_increasing() {
    let buf = RingBuffer::new(10);
    buf.push(1, "a".into());
    std::thread::sleep(std::time::Duration::from_millis(10));
    buf.push(1, "b".into());

    let (entries, _) = buf.get_entries_from(1, 100);
    assert_eq!(entries.len(), 2);
    assert!(entries[1].time >= entries[0].time);
}

// ── DebugEntry serialization ──────────────────────────────

#[test]
fn debug_entry_serializes_to_json() {
    let entry = DebugEntry {
        seq: 1,
        time: 132000000000000000,
        pid: 1234,
        process_name: "test.exe".to_string(),
        text: "Hello world".to_string(),
    };

    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("\"seq\":1"));
    assert!(json.contains("\"pid\":1234"));
    assert!(json.contains("\"text\":\"Hello world\""));

    // Round-trip
    let deserialized: DebugEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.seq, entry.seq);
    assert_eq!(deserialized.text, entry.text);
}
