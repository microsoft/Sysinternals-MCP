//! Windows debug output capture using kernel objects

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::error::{DbgViewError, Result};
use crate::ring_buffer::SharedRingBuffer;

#[cfg(windows)]
mod windows_impl {
    use super::*;

    use windows::Win32::Foundation::{
        CloseHandle, HANDLE, WAIT_OBJECT_0, BOOL,
    };
    use windows::Win32::System::Memory::{
        CreateFileMappingA, MapViewOfFile, UnmapViewOfFile,
        FILE_MAP_READ, FILE_MAP_WRITE, PAGE_READWRITE,
    };
    use windows::Win32::System::Threading::{
        CreateEventA, CreateMutexA, OpenMutexW, ReleaseMutex, SetEvent, WaitForSingleObject,
        SYNCHRONIZATION_ACCESS_RIGHTS,
    };
    use windows::Win32::Security::SECURITY_ATTRIBUTES;
    use windows::core::{PCSTR, PCWSTR};

    // Kernel object names for debug capture (NO prefix = current session only)
    const DBWIN_MUTEX_NAME: &[u8] = b"DBWinMutex\0";
    const DBWIN_BUFFER_NAME: &[u8] = b"DBWIN_BUFFER\0";
    const DBWIN_BUFFER_READY_NAME: &[u8] = b"DBWIN_BUFFER_READY\0";
    const DBWIN_DATA_READY_NAME: &[u8] = b"DBWIN_DATA_READY\0";

    // Wide string versions for OpenMutexW
    const DBWIN_MUTEX_NAME_W: &[u16] = &[
        'D' as u16, 'B' as u16, 'W' as u16, 'i' as u16, 'n' as u16, 
        'M' as u16, 'u' as u16, 't' as u16, 'e' as u16, 'x' as u16, 0
    ];

    // Buffer size (4KB as per Windows specification)
    const DBWIN_BUFFER_SIZE: usize = 4096;

    /// DBWIN_BUFFER structure layout
    #[repr(C)]
    struct DbwinBuffer {
        process_id: u32,
        data: [u8; DBWIN_BUFFER_SIZE - 4],
    }

    /// Windows kernel objects for debug capture
    pub struct DebugCaptureWindows {
        buffer: SharedRingBuffer,
        running: Arc<AtomicBool>,
        capture_thread: Option<JoinHandle<()>>,
    }

    impl DebugCaptureWindows {
        pub fn new(buffer: SharedRingBuffer) -> Result<Self> {
            Ok(Self {
                buffer,
                running: Arc::new(AtomicBool::new(false)),
                capture_thread: None,
            })
        }

        pub fn start(&mut self) -> Result<()> {
            if self.running.load(Ordering::SeqCst) {
                return Err(DbgViewError::CaptureAlreadyRunning);
            }

            self.running.store(true, Ordering::SeqCst);
            let running = self.running.clone();
            let buffer = self.buffer.clone();

            let handle = thread::spawn(move || {
                if let Err(e) = capture_loop(running.clone(), buffer) {
                    tracing::error!("Capture loop error: {}", e);
                }
                running.store(false, Ordering::SeqCst);
            });

            self.capture_thread = Some(handle);
            Ok(())
        }

        pub fn stop(&mut self) -> Result<()> {
            self.running.store(false, Ordering::SeqCst);
            if let Some(handle) = self.capture_thread.take() {
                // The thread will exit on next timeout or signal
                let _ = handle.join();
            }
            Ok(())
        }

        pub fn is_running(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }
    }

    fn capture_loop(running: Arc<AtomicBool>, buffer: SharedRingBuffer) -> Result<()> {
        unsafe {
            // Set up security attributes - use default (no explicit SDDL needed for current session)
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: std::ptr::null_mut(),
                bInheritHandle: BOOL(1),
            };

            let mutex_name = PCSTR::from_raw(DBWIN_MUTEX_NAME.as_ptr());
            let mutex_name_w = PCWSTR::from_raw(DBWIN_MUTEX_NAME_W.as_ptr());
            
            // SYNCHRONIZE = 0x00100000
            const SYNCHRONIZE: u32 = 0x00100000;
            
            // Try to open existing mutex first (like the original C code)
            let mutex = match OpenMutexW(SYNCHRONIZATION_ACCESS_RIGHTS(SYNCHRONIZE), false, mutex_name_w) {
                Ok(h) => h,
                Err(_) => {
                    // Mutex doesn't exist, create it
                    CreateMutexA(Some(&sa), false, mutex_name)
                        .map_err(|e| DbgViewError::KernelObjectCreation {
                            name: "DBWinMutex".to_string(),
                            source: std::io::Error::from_raw_os_error(e.code().0 as i32),
                        })?
                }
            };

            // Create shared memory buffer
            let buffer_name = PCSTR::from_raw(DBWIN_BUFFER_NAME.as_ptr());
            let file_mapping = CreateFileMappingA(
                HANDLE::default(),
                Some(&sa),
                PAGE_READWRITE,
                0,
                DBWIN_BUFFER_SIZE as u32,
                buffer_name,
            )
            .map_err(|e| DbgViewError::KernelObjectCreation {
                name: "DBWIN_BUFFER".to_string(),
                source: std::io::Error::from_raw_os_error(e.code().0 as i32),
            })?;

            let shared_mem = MapViewOfFile(file_mapping, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0);
            if shared_mem.Value.is_null() {
                let _ = CloseHandle(file_mapping);
                let _ = CloseHandle(mutex);
                return Err(DbgViewError::MemoryMapping(std::io::Error::last_os_error()));
            }

            // Create events
            let buffer_ready_name = PCSTR::from_raw(DBWIN_BUFFER_READY_NAME.as_ptr());
            let buffer_ready = CreateEventA(Some(&sa), false, false, buffer_ready_name)
                .map_err(|e| {
                    let _ = UnmapViewOfFile(shared_mem);
                    let _ = CloseHandle(file_mapping);
                    let _ = CloseHandle(mutex);
                    DbgViewError::KernelObjectCreation {
                        name: "DBWIN_BUFFER_READY".to_string(),
                        source: std::io::Error::from_raw_os_error(e.code().0 as i32),
                    }
                })?;

            let data_ready_name = PCSTR::from_raw(DBWIN_DATA_READY_NAME.as_ptr());
            let data_ready = CreateEventA(Some(&sa), false, false, data_ready_name)
                .map_err(|e| {
                    let _ = CloseHandle(buffer_ready);
                    let _ = UnmapViewOfFile(shared_mem);
                    let _ = CloseHandle(file_mapping);
                    let _ = CloseHandle(mutex);
                    DbgViewError::KernelObjectCreation {
                        name: "DBWIN_DATA_READY".to_string(),
                        source: std::io::Error::from_raw_os_error(e.code().0 as i32),
                    }
                })?;

            tracing::info!("Debug capture started");

            // Signal that buffer is ready for first write
            let _ = SetEvent(buffer_ready);

            // Main capture loop
            while running.load(Ordering::SeqCst) {
                // Wait for data with timeout so we can check running flag
                let wait_result = WaitForSingleObject(data_ready, 1000); // 1s timeout

                if wait_result == WAIT_OBJECT_0 {
                    // Data is ready, read it
                    let dbwin_buffer = shared_mem.Value as *const DbwinBuffer;
                    let pid = (*dbwin_buffer).process_id;
                    
                    // Find null terminator in data
                    let data = &(*dbwin_buffer).data;
                    let len = data.iter().position(|&b| b == 0).unwrap_or(data.len());
                    
                    if len > 0 {
                        // Convert to string, handling invalid UTF-8
                        let text = String::from_utf8_lossy(&data[..len]).into_owned();
                        buffer.push(pid, text);
                    }
                    
                    // Signal that buffer is ready for next write
                    let _ = SetEvent(buffer_ready);
                }
                // WAIT_TIMEOUT: just continue loop
            }

            // Cleanup
            tracing::info!("Debug capture stopped");
            let _ = UnmapViewOfFile(shared_mem);
            let _ = CloseHandle(file_mapping);
            let _ = CloseHandle(buffer_ready);
            let _ = CloseHandle(data_ready);
            let _ = ReleaseMutex(mutex);
            let _ = CloseHandle(mutex);
        }

        Ok(())
    }
}

#[cfg(not(windows))]
mod stub_impl {
    use super::*;

    /// Stub implementation for non-Windows platforms
    pub struct DebugCaptureStub {
        buffer: SharedRingBuffer,
        running: AtomicBool,
    }

    impl DebugCaptureStub {
        pub fn new(buffer: SharedRingBuffer) -> Result<Self> {
            Ok(Self {
                buffer,
                running: AtomicBool::new(false),
            })
        }

        pub fn start(&mut self) -> Result<()> {
            Err(DbgViewError::PlatformNotSupported)
        }

        pub fn stop(&mut self) -> Result<()> {
            Ok(())
        }

        pub fn is_running(&self) -> bool {
            false
        }
    }
}

/// Platform-agnostic debug capture wrapper
pub struct DebugCapture {
    #[cfg(windows)]
    inner: windows_impl::DebugCaptureWindows,
    #[cfg(not(windows))]
    inner: stub_impl::DebugCaptureStub,
}

impl DebugCapture {
    /// Create a new debug capture instance
    pub fn new(buffer: SharedRingBuffer) -> Result<Self> {
        #[cfg(windows)]
        let inner = windows_impl::DebugCaptureWindows::new(buffer)?;
        #[cfg(not(windows))]
        let inner = stub_impl::DebugCaptureStub::new(buffer)?;

        Ok(Self { inner })
    }

    /// Start capturing debug output
    pub fn start(&mut self) -> Result<()> {
        self.inner.start()
    }

    /// Stop capturing debug output
    pub fn stop(&mut self) -> Result<()> {
        self.inner.stop()
    }

    /// Check if capture is running
    pub fn is_running(&self) -> bool {
        self.inner.is_running()
    }
}
