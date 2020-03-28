// Copyright (C) 2017-2020 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::avcodec::{
    av_init_packet, moonfire_ffmpeg_packet_alloc, moonfire_ffmpeg_packet_free, AVCodecParameters,
    AVPacket, InputCodecParameters, Packet,
};
use crate::avutil::{Dictionary, Error};
use std::cell::RefCell;
use std::ffi::CStr;
use std::ptr;

//#[link(name = "avformat")]
extern "C" {
    pub(crate) fn avformat_version() -> libc::c_int;

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

    // avformat
    fn moonfire_ffmpeg_fctx_streams(ctx: *const AVFormatContext) -> StreamsLen;
    //fn moonfire_ffmpeg_fctx_open_write(ctx: *mut AVFormatContext,
    //                                   url: *const libc::c_char) -> libc::c_int;

    fn moonfire_ffmpeg_stream_codecpar(stream: *const AVStream) -> *const AVCodecParameters;
    fn moonfire_ffmpeg_stream_duration(stream: *const AVStream) -> i64;
    fn moonfire_ffmpeg_stream_time_base(stream: *const AVStream) -> crate::avutil::Rational;
}

// No ABI stability assumption here; use heap allocation/deallocation and accessors only.
enum AVFormatContext {}
enum AVInputFormat {}
enum AVStream {}

pub struct InputFormatContext {
    ctx: *mut AVFormatContext,
    pkt: RefCell<*mut AVPacket>,
}

impl InputFormatContext {
    pub fn open(source: &CStr, dict: &mut Dictionary) -> Result<Self, Error> {
        let mut ctx = ptr::null_mut();
        Error::wrap(unsafe { avformat_open_input(&mut ctx, source.as_ptr(), ptr::null(), dict) })?;
        let pkt = unsafe { moonfire_ffmpeg_packet_alloc() };
        if pkt.is_null() {
            panic!("malloc failed");
        }
        unsafe { av_init_packet(pkt) };
        Ok(InputFormatContext {
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

    pub fn streams(&self) -> Streams<'_> {
        Streams(unsafe {
            let s = moonfire_ffmpeg_fctx_streams(self.ctx);
            std::slice::from_raw_parts(s.streams, s.len as usize)
        })
    }
}

unsafe impl Send for InputFormatContext {}

impl Drop for InputFormatContext {
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
    pub fn codecpar(&self) -> InputCodecParameters<'_> {
        InputCodecParameters(unsafe { moonfire_ffmpeg_stream_codecpar(self.0).as_ref() }.unwrap())
    }

    pub fn time_base(&self) -> crate::avutil::Rational {
        unsafe { moonfire_ffmpeg_stream_time_base(self.0) }
    }

    pub fn duration(&self) -> i64 {
        unsafe { moonfire_ffmpeg_stream_duration(self.0) }
    }
}
