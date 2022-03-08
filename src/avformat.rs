// Copyright (C) 2017-2020 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::avcodec::{
    av_init_packet, moonfire_ffmpeg_packet_alloc, moonfire_ffmpeg_packet_free, AVCodecParameters,
    AVPacket, InputCodecParameters, Packet,
};
use crate::avutil::{Dictionary, Error};
use std::cell::RefCell;
use std::convert::TryFrom;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::ptr;

//#[link(name = "avformat")]
extern "C" {
    pub(crate) fn avformat_version() -> libc::c_int;
    pub(crate) fn avformat_configuration() -> *mut libc::c_char;

    fn avformat_alloc_context() -> *mut AVFormatContext;

    fn avio_alloc_context(
        buffer: *const u8,
        buffer_size: libc::c_int,
        write_flag: libc::c_int,
        opaque: *const libc::c_void,
        read_packet: Option<
            unsafe extern "C" fn(
                opaque: *const libc::c_void,
                buf: *mut u8,
                buf_size: libc::c_int,
            ) -> libc::c_int,
        >,
        write_packet: Option<
            unsafe extern "C" fn(
                opaque: *const libc::c_void,
                buf: *const u8,
                buf_size: libc::c_int,
            ) -> libc::c_int,
        >,
        seek: Option<
            unsafe extern "C" fn(
                opaque: *const libc::c_void,
                offset: i64,
                whence: libc::c_int,
            ) -> i64,
        >,
    ) -> *mut AVIOContext;
    fn avio_context_free(s: *mut *mut AVIOContext);

    //fn avformat_alloc_output_context2(ctx: *mut *mut AVFormatContext, oformat: *mut AVOutputFormat,
    //                                  format_name: *const libc::c_char,
    //                                  filename: *const libc::c_char) -> libc::c_int;
    fn avformat_open_input(
        ctx: *mut *mut AVFormatContext,
        url: *const libc::c_char,
        fmt: *const AVInputFormat,
        options: *mut Dictionary,
    ) -> libc::c_int;
    fn avformat_close_input(ctx: *mut *mut AVFormatContext);
    fn avformat_find_stream_info(
        ctx: *mut AVFormatContext,
        options: *mut Dictionary,
    ) -> libc::c_int;
    //fn avformat_new_stream(s: *mut AVFormatContext, c: *const AVCodec) -> *mut AVStream;
    //fn avformat_write_header(c: *mut AVFormatContext, opts: *mut *mut AVDictionary) -> libc::c_int;
    fn av_read_frame(ctx: *mut AVFormatContext, p: *mut AVPacket) -> libc::c_int;
    pub(crate) fn av_register_all();
    pub(crate) fn avformat_network_init() -> libc::c_int;
}

//#[link(name = "wrapper")]
extern "C" {
    pub(crate) static moonfire_ffmpeg_compiled_libavformat_version: libc::c_int;

    static moonfire_ffmpeg_avseek_force: libc::c_int;
    static moonfire_ffmpeg_avseek_size: libc::c_int;
    static moonfire_ffmpeg_seek_set: libc::c_int;
    static moonfire_ffmpeg_seek_cur: libc::c_int;
    static moonfire_ffmpeg_seek_end: libc::c_int;

    fn moonfire_ffmpeg_fctx_streams(ctx: *const AVFormatContext) -> StreamsLen;
    //fn moonfire_ffmpeg_fctx_open_write(ctx: *mut AVFormatContext,
    //                                   url: *const libc::c_char) -> libc::c_int;
    //

    fn moonfire_ffmpeg_fctx_set_pb(ctx: *mut AVFormatContext, pb: *mut AVIOContext);

    fn moonfire_ffmpeg_ioctx_set_direct(pb: *mut AVIOContext);

    fn moonfire_ffmpeg_stream_codecpar(stream: *const AVStream) -> *const AVCodecParameters;
    fn moonfire_ffmpeg_stream_duration(stream: *const AVStream) -> i64;
    fn moonfire_ffmpeg_stream_time_base(stream: *const AVStream) -> crate::avutil::Rational;
}

// No ABI stability assumption here; use heap allocation/deallocation and accessors only.
#[repr(C)]
struct AVFormatContext {
    _private: [u8; 0],
}
#[repr(C)]
struct AVIOContext {
    _private: [u8; 0],
}
#[repr(C)]
struct AVInputFormat {
    _private: [u8; 0],
}
#[repr(C)]
struct AVStream {
    _private: [u8; 0],
}

pub struct InputFormatContext<'a> {
    /// When using `InputFormatContext::with_io_context`, `ctx` has a `pb` member which has an
    /// `opaque` referencing `_io_ctx`.
    _io_ctx: PhantomData<&'a mut dyn IoContext>,
    ctx: *mut AVFormatContext,
    pkt: RefCell<*mut AVPacket>,
}

/// Mode argument to `IoContext::seek`.
pub enum Whence {
    /// Return the size (if possible) without actually seeking.
    Size,

    /// Sets a position relative to the start of the file, returning the new position.
    Set,

    /// Sets a position relative to the current position, returning the new position.
    Cur,

    /// Sets a position relative to the end of the file, returning the new position.
    End,
}

/// An implementation of the IO operations needed by libavformat.
/// See ffmpeg's `avio_alloc_context` and `avformat_open_input` for more information.
pub trait IoContext {
    /// Returns true iff this is readable; then `read` should be implemented.
    fn readable(&self) -> bool {
        false
    }

    /// Returns true iff this is writable; then `write` should be implemented.
    fn writable(&self) -> bool {
        false
    }

    /// Returns true iff this is seekable; then `seek` should be implemented.
    fn seekable(&self) -> bool {
        false
    }

    /// Returns true iff the buffer should be skipped; this is a hint; ffmpeg may use the buffer
    /// anyway.
    fn direct(&self) -> bool {
        false
    }

    /// Returns the desired length of the buffer.
    fn buf_len(&self) -> usize;

    /// Reads up to the given number of bytes into `buf`.
    #[allow(unused_variables)]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        Err(Error::enosys())
    }

    /// Writes up to the given number of bytes into `buf`.
    #[allow(unused_variables)]
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        Err(Error::enosys())
    }

    /// Performs the operation described by `whence`.
    /// `force` corresponds to the `AVSEEK_FORCE` flag.
    #[allow(unused_variables)]
    fn seek(&mut self, offset: i64, whence: Whence, force: bool) -> Result<u64, Error> {
        Err(Error::enosys())
    }
}

/// An `IoContext` implementation for an immutable slice.
pub struct SliceIoContext<'a> {
    slice: &'a [u8],
    pos: usize,
}

impl<'a> SliceIoContext<'a> {
    pub fn new(slice: &'a [u8]) -> Self {
        Self { slice, pos: 0 }
    }
}

impl<'a> IoContext for SliceIoContext<'a> {
    fn readable(&self) -> bool {
        true
    }
    fn writable(&self) -> bool {
        false
    }
    fn seekable(&self) -> bool {
        true
    }
    fn direct(&self) -> bool {
        true
    }
    fn buf_len(&self) -> usize {
        std::cmp::min(self.slice.len(), 4096)
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let copy_len = std::cmp::min(buf.len(), self.slice.len() - self.pos);
        buf[0..copy_len].copy_from_slice(&self.slice[self.pos..self.pos + copy_len]);
        self.pos += copy_len;
        Ok(copy_len)
    }
    fn seek(&mut self, offset: i64, whence: Whence, _force: bool) -> Result<u64, Error> {
        let offset = usize::try_from(offset).map_err(|_| Error::invalid_data())?;
        let new_pos = match whence {
            Whence::Size => return Ok(u64::try_from(self.slice.len()).unwrap()),
            Whence::Set => offset,
            Whence::Cur => self
                .pos
                .checked_add(offset)
                .ok_or_else(Error::invalid_data)?,
            Whence::End => self
                .pos
                .checked_add(offset)
                .ok_or_else(Error::invalid_data)?,
        };
        if new_pos > self.slice.len() {
            return Err(Error::invalid_data());
        }
        self.pos = new_pos;
        Ok(u64::try_from(self.pos).unwrap())
    }
}

struct IoContextWrapper<'a> {
    // The opaque pointer passed to the callbacks must be thin, so create a box here so we have a
    // stable address to the fat pointer itself.
    _ctx: Box<&'a mut dyn IoContext>,

    avio_ctx: ptr::NonNull<AVIOContext>,
}

/// Implements the `read_packet` argument to `avio_alloc_context`.
unsafe extern "C" fn ioctx_read_packet(
    opaque: *const libc::c_void,
    buf_data: *mut u8,
    buf_len: libc::c_int,
) -> libc::c_int {
    let ctx: &mut &mut dyn IoContext = &mut *(opaque as *mut &mut dyn IoContext);
    let buf = std::slice::from_raw_parts_mut(buf_data, usize::try_from(buf_len).unwrap());
    match ctx.read(buf) {
        Ok(l) => libc::c_int::try_from(l).unwrap(),
        Err(e) => e.get(),
    }
}

/// Implements the `write_packet` argument to `avio_alloc_context`.
unsafe extern "C" fn ioctx_write_packet(
    opaque: *const libc::c_void,
    buf_data: *const u8,
    buf_len: libc::c_int,
) -> libc::c_int {
    let ctx: &mut &mut dyn IoContext = &mut *(opaque as *mut &mut dyn IoContext);
    let buf = std::slice::from_raw_parts(buf_data, usize::try_from(buf_len).unwrap());
    match ctx.write(buf) {
        Ok(l) => libc::c_int::try_from(l).unwrap(),
        Err(e) => e.get(),
    }
}

/// Implements the `seek_packet` argument to `avio_alloc_context`.
unsafe extern "C" fn ioctx_seek(
    opaque: *const libc::c_void,
    offset: i64,
    whence: libc::c_int,
) -> i64 {
    let ctx: &mut &mut dyn IoContext = &mut *(opaque as *mut &mut dyn IoContext);
    let avseek_force = moonfire_ffmpeg_avseek_force;
    let force = (whence & avseek_force) != 0;
    let w = whence & !avseek_force;
    let whence = if (w & moonfire_ffmpeg_avseek_size) != 0 {
        Whence::Size
    } else if w == moonfire_ffmpeg_seek_set {
        Whence::Set
    } else if w == moonfire_ffmpeg_seek_cur {
        Whence::Cur
    } else if w == moonfire_ffmpeg_seek_end {
        Whence::End
    } else {
        panic!("invalid whence {}", whence);
    };
    match ctx.seek(offset, whence, force) {
        Ok(p) => i64::try_from(p).unwrap(),
        Err(e) => i64::from(e.get()),
    }
}

impl<'a> IoContextWrapper<'a> {
    fn new(ctx: &'a mut dyn IoContext) -> Result<Self, Error> {
        let ctx = Box::new(ctx);
        let buf_len = ctx.buf_len();
        let mut buf = crate::avutil::Alloc::new(buf_len)?;
        let avio_ctx = ptr::NonNull::new(unsafe {
            let opaque: &&mut dyn IoContext = &ctx;
            avio_alloc_context(
                buf.as_ptr() as *const u8,
                i32::try_from(buf_len).unwrap(),
                if ctx.writable() { 1 } else { 0 },
                opaque as *const &mut dyn IoContext as *mut core::ffi::c_void,
                if ctx.readable() {
                    Some(ioctx_read_packet)
                } else {
                    None
                },
                if ctx.writable() {
                    Some(ioctx_write_packet)
                } else {
                    None
                },
                if ctx.seekable() {
                    Some(ioctx_seek)
                } else {
                    None
                },
            )
        })
        .ok_or_else(Error::enomem)?;
        std::mem::forget(buf); // owned by avio_ctx iff alloc is successful.
        if ctx.direct() {
            unsafe { moonfire_ffmpeg_ioctx_set_direct(avio_ctx.as_ptr()) };
        }
        Ok(Self {
            avio_ctx,
            _ctx: ctx,
        })
    }

    fn release(self) -> *mut AVIOContext {
        let p = self.avio_ctx.as_ptr();
        std::mem::forget(self);
        p
    }
}

impl<'a> Drop for IoContextWrapper<'a> {
    fn drop(&mut self) {
        let mut p = self.avio_ctx.as_ptr();
        unsafe { avio_context_free(&mut p) };
    }
}

impl<'a> InputFormatContext<'a> {
    pub fn open(source: &CStr, dict: &mut Dictionary) -> Result<Self, Error> {
        let mut ctx = ptr::null_mut();
        Error::wrap(unsafe { avformat_open_input(&mut ctx, source.as_ptr(), ptr::null(), dict) })?;
        let pkt = unsafe { moonfire_ffmpeg_packet_alloc() };
        if pkt.is_null() {
            unsafe { avformat_close_input(&mut ctx) };
            return Err(Error::enomem());
        }
        unsafe { av_init_packet(pkt) };
        Ok(InputFormatContext {
            ctx,
            _io_ctx: PhantomData,
            pkt: RefCell::new(pkt),
        })
    }

    pub fn with_io_context(
        source: &CStr,
        io_ctx: &'a mut dyn IoContext,
        dict: &mut Dictionary,
    ) -> Result<Self, Error> {
        let wrapper = IoContextWrapper::new(io_ctx)?;
        let mut ctx = unsafe { avformat_alloc_context() };
        if ctx.is_null() {
            return Err(Error::enomem());
        }
        // Note that `ctx` is freed by `avformat_open_input` on failure, including the avio_ctx.
        unsafe { moonfire_ffmpeg_fctx_set_pb(ctx, wrapper.release()) };
        Error::wrap(unsafe { avformat_open_input(&mut ctx, source.as_ptr(), ptr::null(), dict) })?;
        let pkt = unsafe { moonfire_ffmpeg_packet_alloc() };
        if pkt.is_null() {
            unsafe { avformat_close_input(&mut ctx) };
            return Err(Error::enomem());
        }
        unsafe { av_init_packet(pkt) };
        Ok(InputFormatContext {
            _io_ctx: PhantomData,
            ctx,
            pkt: RefCell::new(pkt),
        })
    }

    pub fn find_stream_info(&mut self) -> Result<(), Error> {
        Error::wrap(unsafe { avformat_find_stream_info(self.ctx, ptr::null_mut()) })?;
        Ok(())
    }

    // XXX: non-mut because of lexical lifetime woes in the caller. This is also why we need a
    // RefCell.
    pub fn read_frame(&self) -> Result<Packet<'_>, Error> {
        let pkt = self.pkt.borrow();
        Error::wrap(unsafe { av_read_frame(self.ctx, *pkt) })?;
        Ok(Packet(pkt))
    }

    pub fn streams(&self) -> Streams {
        Streams(unsafe {
            let s = moonfire_ffmpeg_fctx_streams(self.ctx);
            std::slice::from_raw_parts(s.streams, s.len as usize)
        })
    }
}

unsafe impl<'a> Send for InputFormatContext<'a> {}

impl<'a> Drop for InputFormatContext<'a> {
    fn drop(&mut self) {
        unsafe {
            moonfire_ffmpeg_packet_free(*self.pkt.borrow());
            avformat_close_input(&mut self.ctx);
        }
    }
}

// matches moonfire_ffmpeg_streams_len
#[repr(C)]
struct StreamsLen {
    streams: *const *const AVStream,
    len: libc::size_t,
}

pub struct Streams<'owner>(&'owner [*const AVStream]);

impl<'owner> Streams<'owner> {
    pub fn get(&self, i: usize) -> InputStream<'owner> {
        InputStream(unsafe { self.0[i].as_ref() }.unwrap())
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct InputStream<'o>(&'o AVStream);

impl<'o> InputStream<'o> {
    pub fn codecpar(&self) -> InputCodecParameters<'o> {
        InputCodecParameters(unsafe { moonfire_ffmpeg_stream_codecpar(self.0).as_ref() }.unwrap())
    }

    pub fn time_base(&self) -> crate::avutil::Rational {
        unsafe { moonfire_ffmpeg_stream_time_base(self.0) }
    }

    pub fn duration(&self) -> i64 {
        unsafe { moonfire_ffmpeg_stream_duration(self.0) }
    }
}

#[cfg(test)]
mod test {
    use cstr::cstr;

    fn with_packets<F>(ctx: &mut super::InputFormatContext, mut f: F)
    where
        F: FnMut(crate::avcodec::Packet),
    {
        loop {
            let pkt = match ctx.read_frame() {
                Err(e) if e.is_eof() => break,
                Err(e) => panic!("{}", e),
                Ok(p) => p,
            };
            f(pkt);
        }
    }

    #[test]
    fn file() {
        crate::Ffmpeg::new();
        let mut dict = crate::avutil::Dictionary::new();
        let mut ctx =
            super::InputFormatContext::open(cstr!("src/testdata/clip.mp4"), &mut dict).unwrap();
        let mut pts = Vec::new();
        with_packets(&mut ctx, |pkt| pts.push(pkt.pts().unwrap()));
        assert_eq!(pts, &[0, 29700, 59400, 90000, 119700, 149400]);
    }

    // Directly reference it as a slice.
    #[test]
    fn slice() {
        crate::Ffmpeg::new();
        let mut dict = crate::avutil::Dictionary::new();
        let mut io_ctx = super::SliceIoContext::new(include_bytes!("testdata/clip.mp4"));
        let mut ctx =
            super::InputFormatContext::with_io_context(cstr!(""), &mut io_ctx, &mut dict).unwrap();
        let mut pts = Vec::new();
        with_packets(&mut ctx, |pkt| pts.push(pkt.pts().unwrap()));
        assert_eq!(pts, &[0, 29700, 59400, 90000, 119700, 149400]);
    }
}
