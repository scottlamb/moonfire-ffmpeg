// Copyright (C) 2021 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use libc::{c_char, c_int};
use log::info;
use parking_lot::Once;
use std::convert::TryFrom;
use std::ffi::CStr;
use std::fmt::{self, Write};

static START: Once = Once::new();

pub mod avcodec;
pub mod avformat;
pub mod avutil;
#[cfg(feature = "swscale")]
pub mod swscale;

pub use avutil::Error;

type RustLogCallback = extern "C" fn(
    avc_item_name: *const c_char,
    avc: *const libc::c_void,
    level: libc::c_int,
    fmt: *const c_char,
    vl: *mut libc::c_void,
);

//#[link(name = "wrapper")]
extern "C" {
    static moonfire_ffmpeg_version: *const libc::c_char;

    fn moonfire_ffmpeg_init(cb: RustLogCallback);

    fn moonfire_ffmpeg_vsnprintf(
        buf: *mut u8,
        size: usize,
        fmt: *const c_char,
        vl: *mut libc::c_void,
    ) -> c_int;
}

pub struct Ffmpeg {}

#[derive(Copy, Clone)]
struct Version(libc::c_int);

impl Version {
    fn major(self) -> libc::c_int {
        (self.0 >> 16) & 0xFF
    }
    fn minor(self) -> libc::c_int {
        (self.0 >> 8) & 0xFF
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}",
            (self.0 >> 16) & 0xFF,
            (self.0 >> 8) & 0xFF,
            self.0 & 0xFF
        )
    }
}

struct Library {
    name: &'static str,
    compiled: Version,
    running: Version,
    configuration: &'static CStr,
}

impl Library {
    fn new(
        name: &'static str,
        compiled: libc::c_int,
        running: libc::c_int,
        configuration: &'static CStr,
    ) -> Self {
        Library {
            name,
            compiled: Version(compiled),
            running: Version(running),
            configuration,
        }
    }

    fn is_compatible(&self) -> bool {
        self.running.major() == self.compiled.major()
            && self.running.minor() >= self.compiled.minor()
    }
}

impl fmt::Display for Library {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Write in the same order as ffmpeg's PRINT_LIB_INFO to reduce confusion:
        // compiled, then running, then configuration.
        write!(
            f,
            "{}: compiled={} running={} configuration={:?}",
            self.name, self.compiled, self.running, self.configuration
        )
    }
}

// Thread-local buffer for av_log.
//
// ffmpeg's av_log calls aren't actually 1-1 with log messages. When it calls
// it without a trailing newline, it's building up a log message for later.
// ffmpeg doesn't use a thread-local buffer, so if two threads' messages overlap,
// it will produce weird results. But we might as well do this properly.
//
// There's one other behavior difference: ffmpeg uses the info from the first
// call of the message, where we use the last one. This avoids having to do
// an extra allocation. It should be the same result.
thread_local! {
    static LOG_BUF: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(Vec::with_capacity(1024));
}

/// Appends the given `fmt` and `vl` to `buf` using `vsnprintf`.
unsafe fn append_vprintf(buf: &mut Vec<u8>, fmt: *const libc::c_char, vl: *mut libc::c_void) {
    let left = buf.capacity() - buf.len();
    let ret = moonfire_ffmpeg_vsnprintf(buf.as_mut_ptr_range().end, left, fmt, vl);
    let ret = match usize::try_from(ret) {
        Ok(r) => r,
        Err(_) => {
            buf.extend(b"(vsnprintf failed)");
            return;
        }
    };
    if ret >= left {
        // Buffer is too small to put in the contents (with the trailing NUL,
        // which vsnprintf insists on). Now we know the correct size.
        buf.reserve(ret + 1);
        let ret2 = moonfire_ffmpeg_vsnprintf(buf.as_mut_ptr_range().end, ret + 1, fmt, vl);
        assert_eq!(
            usize::try_from(ret2).expect("2nd vsnprintf should succeed"),
            ret
        );
    }
    buf.set_len(buf.len() + ret);
}

/// Log callback which sends `av_log_default_callback`-like payloads into the
/// log crate, turning ffmpeg's `avc_item_name` into a module path and ffmpeg's
/// levels into log crate levels.
extern "C" fn log_callback(
    avc_item_name: *const c_char,
    avc: *const libc::c_void,
    level: libc::c_int,
    fmt: *const c_char,
    vl: *mut libc::c_void,
) {
    let log_level = avutil::convert_level(level);

    // Fast path so trace calls don't allocate when trace isn't enabled anywhere.
    if log::max_level()
        .to_level()
        .map(|l| l < log_level)
        .unwrap_or(true)
    {
        return;
    }
    let avc_item_name = if avc_item_name.is_null() {
        "null"
    } else {
        unsafe { CStr::from_ptr(avc_item_name) }
            .to_str()
            .unwrap_or("bad_utf8")
    };
    let target = format!("moonfire_ffmpeg::{}", avc_item_name);
    let metadata = log::Metadata::builder()
        .level(avutil::convert_level(level))
        .target(&target)
        .build();
    let logger = log::logger();
    if !logger.enabled(&metadata) {
        return;
    }

    LOG_BUF.with(move |b| {
        unsafe { log_callback_inner(&mut *b.borrow_mut(), logger, metadata, avc, fmt, vl) };
    });
}

// Portion of log_callback_int that needs the thread-local data.
unsafe fn log_callback_inner(
    buf: &mut Vec<u8>,
    logger: &dyn log::Log,
    metadata: log::Metadata,
    avc: *const libc::c_void,
    fmt: *const c_char,
    vl: *mut libc::c_void,
) {
    append_vprintf(buf, fmt, vl);

    if !buf
        .last()
        .map(|&b| b == b'\r' || b == b'\n')
        .unwrap_or(false)
    {
        return; // save for next time.
    }

    // ffmpeg log lines apparently sometimes have these low-ASCII control characters.
    // av_log_default_callback "sanitizes" them; match its behavior.
    for c in buf.iter_mut() {
        if *c < 0x08 || (*c > 0x0D && *c < 0x20) {
            *c = b'?';
        }
    }

    let s = String::from_utf8_lossy(&buf[0..buf.len() - 1]);
    let target = metadata.target();
    logger.log(
        &log::RecordBuilder::new()
            .args(format_args!("{:?}: {}", avc, &s))
            .metadata(metadata)
            .module_path(Some(target))
            .build(),
    );
    buf.clear();
}

impl Ffmpeg {
    pub fn new() -> Ffmpeg {
        START.call_once(|| unsafe {
            // Initialize the lock and log callbacks before printing the libraries, because
            // avutil_version() sometimes calls av_log().
            moonfire_ffmpeg_init(log_callback);

            let libs = &[
                Library::new(
                    "avutil",
                    avutil::moonfire_ffmpeg_compiled_libavutil_version,
                    avutil::avutil_version(),
                    CStr::from_ptr(avutil::avutil_configuration()),
                ),
                Library::new(
                    "avcodec",
                    avcodec::moonfire_ffmpeg_compiled_libavcodec_version,
                    avcodec::avcodec_version(),
                    CStr::from_ptr(avcodec::avcodec_configuration()),
                ),
                Library::new(
                    "avformat",
                    avformat::moonfire_ffmpeg_compiled_libavformat_version,
                    avformat::avformat_version(),
                    CStr::from_ptr(avformat::avformat_configuration()),
                ),
                #[cfg(feature = "swscale")]
                Library::new(
                    "swscale",
                    swscale::moonfire_ffmpeg_compiled_libswscale_version,
                    swscale::swscale_version(),
                ),
            ];
            let mut msg = format!(
                "\ncompiled={:?} running={:?}",
                CStr::from_ptr(moonfire_ffmpeg_version),
                CStr::from_ptr(avutil::av_version_info())
            );
            let mut compatible = true;
            for l in libs {
                write!(&mut msg, "\n{}", l).unwrap();
                if !l.is_compatible() {
                    compatible = false;
                    msg.push_str(" <- not ABI-compatible!");
                }
            }
            if !compatible {
                panic!("Incompatible ffmpeg versions:{}", msg);
            }
            avformat::av_register_all();
            if avformat::avformat_network_init() < 0 {
                panic!("avformat_network_init failed");
            }
            info!("Initialized ffmpeg. Versions:{}", msg);
        });
        Ffmpeg {}
    }
}

#[cfg(test)]
mod tests {
    use crate::avutil;
    use cstr::*;
    use parking_lot::Mutex;

    /// Just tests that this doesn't crash with an ABI compatibility error.
    #[test]
    fn test_init() {
        super::Ffmpeg::new();
    }

    #[test]
    fn test_is_compatible() {
        // compiled major/minor/patch, running major/minor/patch, expected compatible
        use ::libc::c_int;
        struct Test(c_int, c_int, c_int, c_int, c_int, c_int, bool);

        let tests = &[
            Test(55, 1, 2, 55, 1, 2, true),  // same version, compatible
            Test(55, 1, 2, 55, 2, 1, true),  // newer minor version, compatible
            Test(55, 1, 3, 55, 1, 2, true),  // older patch version, compatible (but weird)
            Test(55, 2, 2, 55, 1, 2, false), // older minor version, incompatible
            Test(55, 1, 2, 56, 1, 2, false), // newer major version, incompatible
            Test(56, 1, 2, 55, 1, 2, false), // older major version, incompatible
        ];

        for t in tests {
            let l = super::Library::new(
                "avutil",
                (t.0 << 16) | (t.1 << 8) | t.2,
                (t.3 << 16) | (t.4 << 8) | t.5,
                cstr!(""),
            );
            assert!(l.is_compatible() == t.6, "{} expected={}", l, t.6);
        }
    }

    struct DummyLogger(Mutex<Vec<String>>);

    impl log::Log for DummyLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            let mut l = self.0.lock();
            l.push(format!(
                "{}: {}: {}",
                record.level(),
                record.target(),
                record.args()
            ));
        }
        fn flush(&self) {}
    }

    #[test]
    fn test_logging() {
        super::Ffmpeg::new();
        let logger = Box::leak(Box::new(DummyLogger(Mutex::new(Vec::new()))));
        log::set_logger(logger).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
        unsafe {
            avutil::av_log(
                std::ptr::null(),
                avutil::AV_LOG_INFO,
                cstr!("foo %d\n").as_ptr(),
                42 as i32,
            );
            avutil::av_log(
                std::ptr::null(),
                avutil::AV_LOG_INFO,
                cstr!("partial ").as_ptr(),
                1,
            );
            avutil::av_log(
                std::ptr::null(),
                avutil::AV_LOG_INFO,
                cstr!("log\n").as_ptr(),
                1,
            );
            avutil::av_log(
                std::ptr::null(),
                avutil::AV_LOG_INFO,
                cstr!("bar\n").as_ptr(),
            );
        };
        let l = logger.0.lock();
        println!("{:?}", &l[..]);
        assert_eq!(l.len(), 3);
        assert_eq!(&l[0][..], "INFO: moonfire_ffmpeg::null: 0x0: foo 42");
        assert_eq!(&l[1][..], "INFO: moonfire_ffmpeg::null: 0x0: partial log");
        assert_eq!(&l[2][..], "INFO: moonfire_ffmpeg::null: 0x0: bar");
    }
}
