// Copyright (C) 2021 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use libc::{c_char, c_int};
use log::info;
use parking_lot::Once;
use std::{
    convert::TryInto,
    ffi::CStr,
    fmt::{self, Write},
    mem::MaybeUninit,
};

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
    let mut buf: [MaybeUninit<u8>; 1024] = unsafe { MaybeUninit::uninit().assume_init() };
    let buf = unsafe {
        let ret = moonfire_ffmpeg_vsnprintf(buf[0].as_mut_ptr(), buf.len(), fmt, vl);
        std::slice::from_raw_parts_mut(
            buf[0].as_mut_ptr(),
            std::cmp::min(ret.try_into().unwrap(), buf.len() - 1),
        )
    };

    // ffmpeg log lines apparently sometimes have these low-ASCII control characters.
    // av_log_default_callback "sanitizes" them; match its behavior.
    for c in buf.iter_mut() {
        if *c < 0x08 || (*c > 0x0D && *c < 0x20) {
            *c = b'?';
        }
    }

    // av_log calls aren't quite one-to-one with log lines. If they don't have
    // a trailing newline, the following call gets appended on the same line
    // with no prefix. This is a not-very-threadsafe behavior and doesn't seem
    // to come up much in practice. Just treat them all as individual log lines
    // for now, stripping off the trailing newline if it's present (usually).
    let mut buf: &[u8] = buf;
    if buf
        .last()
        .map(|&b| b == b'\r' || b == b'\n')
        .unwrap_or(false)
    {
        buf = &buf[0..buf.len() - 1];
    }

    let buf = String::from_utf8_lossy(buf);
    logger.log(
        &log::RecordBuilder::new()
            .args(format_args!("{:?}: {}", avc, buf))
            .metadata(metadata)
            .module_path(Some(&target))
            .build(),
    );
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
                std::ffi::CStr::from_bytes_with_nul(&[0]).unwrap(),
            );
            assert!(l.is_compatible() == t.6, "{} expected={}", l, t.6);
        }
    }
}
