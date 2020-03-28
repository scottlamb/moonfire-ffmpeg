// Copyright (C) 2017-2020 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::TryFrom;
use std::ffi::CStr;
use std::ptr;

//#[link(name = "avutil")]
extern "C" {
    pub(crate) fn avutil_version() -> libc::c_int;
    fn av_strerror(e: libc::c_int, b: *mut libc::c_char, s: libc::size_t) -> libc::c_int;
    fn av_dict_count(d: *const AVDictionary) -> libc::c_int;
    fn av_dict_get(
        d: *const AVDictionary,
        key: *const libc::c_char,
        prev: *mut AVDictionaryEntry,
        flags: libc::c_int,
    ) -> *mut AVDictionaryEntry;
    fn av_dict_set(
        d: *mut *mut AVDictionary,
        key: *const libc::c_char,
        value: *const libc::c_char,
        flags: libc::c_int,
    ) -> libc::c_int;
    fn av_dict_free(d: *mut *mut AVDictionary);
    fn av_frame_alloc() -> *mut AVFrame;
    fn av_frame_free(f: *mut *mut AVFrame);
    fn av_get_pix_fmt_name(fmt: libc::c_int) -> *const libc::c_char;
}

//#[link(name = "wrapper")]
extern "C" {
    pub(crate) static moonfire_ffmpeg_compiled_libavutil_version: libc::c_int;
    static moonfire_ffmpeg_av_dict_ignore_suffix: libc::c_int;
    pub(crate) static moonfire_ffmpeg_av_nopts_value: i64;

    static moonfire_ffmpeg_averror_eof: libc::c_int;
    static moonfire_ffmpeg_averror_enomem: libc::c_int;
    static moonfire_ffmpeg_averror_decoder_not_found: libc::c_int;
    static moonfire_ffmpeg_averror_unknown: libc::c_int;

    static moonfire_ffmpeg_avmedia_type_video: libc::c_int;

    static moonfire_ffmpeg_pix_fmt_rgb24: libc::c_int;
    static moonfire_ffmpeg_pix_fmt_bgr24: libc::c_int;

    fn moonfire_ffmpeg_frame_image_alloc(
        f: *mut AVFrame,
        dims: *const ImageDimensions,
    ) -> libc::c_int;
    pub(crate) fn moonfire_ffmpeg_frame_stuff(frame: *const AVFrame, stuff: *mut FrameStuff);
}

// No accessors here; seems reasonable to assume ABI stability of this simple struct.
#[repr(C)]
struct AVDictionaryEntry {
    key: *mut libc::c_char,
    value: *mut libc::c_char,
}

// Likewise, seems reasonable to assume this struct has a stable ABI.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct Rational {
    // aka AVRational
    pub num: libc::c_int,
    pub den: libc::c_int,
}

// No ABI stability assumption here; use heap allocation/deallocation and accessors only.
enum AVDictionary {}
pub(crate) enum AVFrame {}

#[repr(C)]
pub(crate) struct FrameStuff {
    dims: ImageDimensions,
    pub(crate) data: *const *mut u8,
    pub(crate) linesizes: *const libc::c_int,
    pts: i64,
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct VideoParameters {
    width: libc::c_int,
    height: libc::c_int,
    sample_aspect_ratio: Rational,
    pix_fmt: PixelFormat,
    time_base: Rational,
}

#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct MediaType(libc::c_int);

impl MediaType {
    pub fn is_video(self) -> bool {
        self.0 == unsafe { moonfire_ffmpeg_avmedia_type_video }
    }
}

pub struct VideoFrame {
    pub(crate) frame: ptr::NonNull<AVFrame>,
    pub(crate) stuff: FrameStuff,
}

pub struct Plane<'f> {
    pub data: &'f [u8],
    pub linesize: usize,
    pub width: usize,
    pub height: usize,
}

impl VideoFrame {
    /// Creates a new `VideoFrame` which is empty: no allocated storage (reference-counted or
    /// otherwise). Can be filled via `DecodeContext::decode_video`.
    pub fn empty() -> Result<Self, Error> {
        let frame = ptr::NonNull::new(unsafe { av_frame_alloc() }).ok_or_else(Error::enomem)?;
        Ok(VideoFrame {
            frame,
            stuff: FrameStuff {
                dims: ImageDimensions {
                    width: 0,
                    height: 0,
                    pix_fmt: PixelFormat(-1),
                },
                data: ptr::null(),
                linesizes: ptr::null(),
                pts: 0,
            },
        })
    }

    /// Creates a new `VideoFrame` with an owned (not reference-counted) buffer of the specified
    /// dimensions.
    pub fn owned(dims: ImageDimensions) -> Result<Self, Error> {
        let mut frame = VideoFrame::empty()?;
        Error::wrap(unsafe { moonfire_ffmpeg_frame_image_alloc(frame.frame.as_mut(), &dims) })?;
        unsafe { moonfire_ffmpeg_frame_stuff(frame.frame.as_ptr(), &mut frame.stuff) };
        Ok(frame)
    }

    pub fn plane(&self, plane: usize) -> Plane {
        assert!(plane < 8);
        let plane_off = isize::try_from(plane).unwrap();
        let d = unsafe { *self.stuff.data.offset(plane_off) };
        let l = unsafe { *self.stuff.linesizes.offset(plane_off) };
        assert!(!d.is_null());
        assert!(l > 0);
        let l = l as usize;
        let width = self.stuff.dims.width as usize;
        let height = self.stuff.dims.height as usize;
        Plane {
            data: unsafe { std::slice::from_raw_parts(d, l * height) },
            linesize: l,
            width,
            height,
        }
    }

    pub fn dims(&self) -> ImageDimensions {
        self.stuff.dims
    }
    pub fn pts(&self) -> i64 {
        self.stuff.pts
    }
}

impl Drop for VideoFrame {
    fn drop(&mut self) {
        unsafe {
            let mut f = self.frame.as_ptr();
            av_frame_free(&mut f);
            // This leaves self.frame dangling, but it's being dropped.
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct PixelFormat(libc::c_int);

impl PixelFormat {
    pub fn rgb24() -> Self {
        PixelFormat(unsafe { moonfire_ffmpeg_pix_fmt_rgb24 })
    }
    pub fn bgr24() -> Self {
        PixelFormat(unsafe { moonfire_ffmpeg_pix_fmt_bgr24 })
    }
}

impl std::fmt::Debug for PixelFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "PixelFormat({} /* {} */)", self.0, self)
    }
}

impl std::fmt::Display for PixelFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = unsafe {
            let n = av_get_pix_fmt_name(self.0);
            if n.is_null() {
                return write!(f, "PixelFormat({})", self.0);
            }
            CStr::from_ptr(n)
        };
        f.write_str(&s.to_string_lossy())
    }
}

#[derive(Copy, Clone)]
pub struct Error(libc::c_int);

impl Error {
    pub fn eof() -> Self {
        Error(unsafe { moonfire_ffmpeg_averror_eof })
    }
    pub fn enomem() -> Self {
        Error(unsafe { moonfire_ffmpeg_averror_enomem })
    }
    pub fn unknown() -> Self {
        Error(unsafe { moonfire_ffmpeg_averror_unknown })
    }
    pub fn decoder_not_found() -> Self {
        Error(unsafe { moonfire_ffmpeg_averror_decoder_not_found })
    }

    /// Wraps the given return code as a Result: positive values are propagated through; negative
    /// values are turned into an `Error`.
    pub(crate) fn wrap(raw: libc::c_int) -> Result<libc::c_int, Error> {
        if raw < 0 {
            return Err(Error(raw));
        }
        Ok(raw)
    }

    pub fn is_eof(self) -> bool {
        self.0 == unsafe { moonfire_ffmpeg_averror_eof }
    }
}

impl std::error::Error for Error {}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Error({} /* {} */)", self.0, self)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        const ARRAYLEN: usize = 64;
        let mut buf = [0; ARRAYLEN];
        let s = unsafe {
            // Note av_strerror uses strlcpy, so it guarantees a trailing NUL byte.
            av_strerror(self.0, buf.as_mut_ptr(), ARRAYLEN);
            CStr::from_ptr(buf.as_ptr())
        };
        f.write_str(&s.to_string_lossy())
    }
}

#[repr(transparent)]
pub struct Dictionary(*mut AVDictionary);

impl Dictionary {
    pub fn new() -> Dictionary {
        Dictionary(ptr::null_mut())
    }
    pub fn size(&self) -> usize {
        (unsafe { av_dict_count(self.0) }) as usize
    }
    pub fn empty(&self) -> bool {
        self.size() == 0
    }
    pub fn set(&mut self, key: &CStr, value: &CStr) -> Result<(), Error> {
        Error::wrap(unsafe { av_dict_set(&mut self.0, key.as_ptr(), value.as_ptr(), 0) })?;
        Ok(())
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Dictionary {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut ent = ptr::null_mut();
        let mut first = true;
        loop {
            unsafe {
                let c = 0;
                ent = av_dict_get(self.0, &c, ent, moonfire_ffmpeg_av_dict_ignore_suffix);
                if ent.is_null() {
                    break;
                }
                if first {
                    first = false;
                } else {
                    write!(f, ", ")?;
                }
                write!(
                    f,
                    "{}={}",
                    CStr::from_ptr((*ent).key).to_string_lossy(),
                    CStr::from_ptr((*ent).value).to_string_lossy()
                )?;
            }
        }
        Ok(())
    }
}

impl Drop for Dictionary {
    fn drop(&mut self) {
        unsafe { av_dict_free(&mut self.0) }
    }
}

// Must match moonfire_ffmpeg_image_dimensions.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct ImageDimensions {
    pub width: libc::c_int,
    pub height: libc::c_int,
    pub pix_fmt: PixelFormat,
}

impl std::fmt::Display for ImageDimensions {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}x{}/{}", self.width, self.height, self.pix_fmt)
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use std::ffi::CString;

    #[test]
    fn test_error() {
        let eof_formatted = format!("{}", Error::eof());
        assert!(
            eof_formatted.contains("End of file"),
            "eof formatted is: {}",
            eof_formatted
        );
        let eof_debug = format!("{:?}", Error::eof());
        assert!(
            eof_debug.contains("End of file"),
            "eof debug is: {}",
            eof_debug
        );

        // Errors should be round trippable to a CString. (This will fail if they contain NUL
        // bytes.)
        CString::new(eof_formatted).unwrap();
    }
}
