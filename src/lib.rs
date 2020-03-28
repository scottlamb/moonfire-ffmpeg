// Copyright (C) 2017-2020 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use log::info;
use parking_lot::Once;
use std::fmt::{self, Write};

static START: Once = Once::new();

pub mod avcodec;
pub mod avformat;
pub mod avutil;
#[cfg(feature = "swscale")]
pub mod swscale;

pub use avutil::Error;

//#[link(name = "wrapper")]
extern "C" {
    fn moonfire_ffmpeg_init();
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
}

impl Library {
    fn new(name: &'static str, compiled: libc::c_int, running: libc::c_int) -> Self {
        Library {
            name,
            compiled: Version(compiled),
            running: Version(running),
        }
    }

    fn is_compatible(&self) -> bool {
        self.running.major() == self.compiled.major()
            && self.running.minor() >= self.compiled.minor()
    }
}

impl fmt::Display for Library {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}: running={} compiled={}",
            self.name, self.running, self.compiled
        )
    }
}

impl Ffmpeg {
    pub fn new() -> Ffmpeg {
        START.call_once(|| unsafe {
            let libs = &[
                Library::new(
                    "avutil",
                    avutil::moonfire_ffmpeg_compiled_libavutil_version,
                    avutil::avutil_version(),
                ),
                Library::new(
                    "avcodec",
                    avcodec::moonfire_ffmpeg_compiled_libavcodec_version,
                    avcodec::avcodec_version(),
                ),
                Library::new(
                    "avformat",
                    avformat::moonfire_ffmpeg_compiled_libavformat_version,
                    avformat::avformat_version(),
                ),
                #[cfg(feature = "swscale")]
                Library::new(
                    "swscale",
                    swscale::moonfire_ffmpeg_compiled_libswscale_version,
                    swscale::swscale_version(),
                ),
            ];
            let mut msg = String::new();
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
            moonfire_ffmpeg_init();
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
    use super::Error;
    use std::ffi::CString;

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
            );
            assert!(l.is_compatible() == t.6, "{} expected={}", l, t.6);
        }
    }
}
