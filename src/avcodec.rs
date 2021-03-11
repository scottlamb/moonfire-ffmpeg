// Copyright (C) 2017-2020 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::avutil::{
    moonfire_ffmpeg_frame_stuff, AVFrame, Dictionary, Error, ImageDimensions, MediaType,
    PixelFormat, Rational, VideoFrame,
};
use std::cell::Ref;
use std::ptr;

//#[link(name = "avcodec")]
extern "C" {
    pub(crate) fn avcodec_version() -> libc::c_int;
    pub(crate) fn avcodec_configuration() -> *mut libc::c_char;
    fn avcodec_alloc_context3(codec: *const AVCodec) -> *mut AVCodecContext;
    fn avcodec_decode_video2(
        ctx: *const AVCodecContext,
        picture: *mut AVFrame,
        got_picture_ptr: *mut libc::c_int,
        pkt: *const AVPacket,
    ) -> libc::c_int;
    fn avcodec_get_name(codec_id: libc::c_int) -> *const libc::c_char;
    fn avcodec_find_decoder(codec_id: libc::c_int) -> *const AVCodec;
    fn avcodec_find_encoder(codec_id: libc::c_int) -> *const AVCodec;
    fn avcodec_free_context(ctx: *mut *mut AVCodecContext);
    fn avcodec_open2(
        ctx: *mut AVCodecContext,
        codec: *const AVCodec,
        options: *mut crate::avutil::Dictionary,
    ) -> libc::c_int;
    fn avcodec_parameters_to_context(
        ctx: *mut AVCodecContext,
        par: *const AVCodecParameters,
    ) -> libc::c_int;
    pub(crate) fn av_init_packet(p: *mut AVPacket);
    fn av_packet_unref(p: *mut AVPacket);
}

//#[link(name = "wrapper")]
extern "C" {
    pub(crate) static moonfire_ffmpeg_compiled_libavcodec_version: libc::c_int;

    static moonfire_ffmpeg_av_codec_id_aac: libc::c_int;
    static moonfire_ffmpeg_av_codec_id_h264: libc::c_int;

    fn moonfire_ffmpeg_codecpar_codec_id(ctx: *const AVCodecParameters) -> CodecId;
    fn moonfire_ffmpeg_codecpar_codec_type(ctx: *const AVCodecParameters) -> MediaType;
    fn moonfire_ffmpeg_codecpar_dims(ctx: *const AVCodecParameters) -> ImageDimensions;
    fn moonfire_ffmpeg_codecpar_extradata(ctx: *const AVCodecParameters) -> DataLen;

    fn moonfire_ffmpeg_cctx_codec_id(ctx: *const AVCodecContext) -> CodecId;
    fn moonfire_ffmpeg_cctx_codec_type(ctx: *const AVCodecContext) -> MediaType;
    fn moonfire_ffmpeg_cctx_pix_fmt(ctx: *const AVCodecContext) -> PixelFormat;
    fn moonfire_ffmpeg_cctx_height(ctx: *const AVCodecContext) -> libc::c_int;
    fn moonfire_ffmpeg_cctx_width(ctx: *const AVCodecContext) -> libc::c_int;
    fn moonfire_ffmpeg_cctx_params(ctx: *const AVCodecContext, p: *mut VideoParameters);
    fn moonfire_ffmpeg_cctx_set_params(ctx: *mut AVCodecContext, p: *const VideoParameters);

    pub(crate) fn moonfire_ffmpeg_packet_alloc() -> *mut AVPacket;
    pub(crate) fn moonfire_ffmpeg_packet_free(p: *mut AVPacket);
    fn moonfire_ffmpeg_packet_is_key(p: *const AVPacket) -> bool;
    fn moonfire_ffmpeg_packet_pts(p: *const AVPacket) -> i64;
    fn moonfire_ffmpeg_packet_dts(p: *const AVPacket) -> i64;
    fn moonfire_ffmpeg_packet_duration(p: *const AVPacket) -> libc::c_int;
    fn moonfire_ffmpeg_packet_set_pts(p: *mut AVPacket, pts: i64);
    fn moonfire_ffmpeg_packet_set_dts(p: *mut AVPacket, dts: i64);
    fn moonfire_ffmpeg_packet_set_duration(p: *mut AVPacket, dur: libc::c_int);
    fn moonfire_ffmpeg_packet_data(p: *const AVPacket) -> DataLen;
    fn moonfire_ffmpeg_packet_stream_index(p: *const AVPacket) -> libc::c_uint;
}

// No ABI stability assumption here; use heap allocation/deallocation and accessors only.
#[repr(C)]
struct AVCodec {
    _private: [u8; 0],
}
#[repr(C)]
pub struct AVCodecContext {
    _private: [u8; 0],
}
#[repr(C)]
pub struct AVCodecParameters {
    _private: [u8; 0],
}
#[repr(C)]
pub(crate) struct AVPacket {
    _private: [u8; 0],
}

impl AVCodecContext {
    pub fn width(&self) -> libc::c_int {
        unsafe { moonfire_ffmpeg_cctx_width(self) }
    }
    pub fn height(&self) -> libc::c_int {
        unsafe { moonfire_ffmpeg_cctx_height(self) }
    }
    pub fn pix_fmt(&self) -> PixelFormat {
        unsafe { moonfire_ffmpeg_cctx_pix_fmt(self) }
    }
    pub fn codec_id(&self) -> CodecId {
        unsafe { moonfire_ffmpeg_cctx_codec_id(self) }
    }
    pub fn codec_type(&self) -> MediaType {
        unsafe { moonfire_ffmpeg_cctx_codec_type(self) }
    }
    pub fn params(&self) -> VideoParameters {
        let mut p = std::mem::MaybeUninit::uninit();
        unsafe {
            moonfire_ffmpeg_cctx_params(self, p.as_mut_ptr());
            p.assume_init()
        }
    }
}

// matches moonfire_ffmpeg_data_len
#[repr(C)]
struct DataLen {
    data: *const u8,
    len: libc::size_t,
}

pub struct Packet<'i>(pub(crate) Ref<'i, *mut AVPacket>);

impl<'i> Packet<'i> {
    pub fn is_key(&self) -> bool {
        unsafe { moonfire_ffmpeg_packet_is_key(*self.0) }
    }
    pub fn pts(&self) -> Option<i64> {
        match unsafe { moonfire_ffmpeg_packet_pts(*self.0) } {
            v if v == unsafe { crate::avutil::moonfire_ffmpeg_av_nopts_value } => None,
            v => Some(v),
        }
    }
    pub fn set_pts(&mut self, pts: Option<i64>) {
        let real_pts = match pts {
            None => unsafe { crate::avutil::moonfire_ffmpeg_av_nopts_value },
            Some(v) => v,
        };
        unsafe {
            moonfire_ffmpeg_packet_set_pts(*self.0, real_pts);
        }
    }
    pub fn dts(&self) -> i64 {
        unsafe { moonfire_ffmpeg_packet_dts(*self.0) }
    }
    pub fn set_dts(&mut self, dts: i64) {
        unsafe {
            moonfire_ffmpeg_packet_set_dts(*self.0, dts);
        }
    }
    pub fn duration(&self) -> i32 {
        unsafe { moonfire_ffmpeg_packet_duration(*self.0) }
    }
    pub fn set_duration(&mut self, dur: i32) {
        unsafe { moonfire_ffmpeg_packet_set_duration(*self.0, dur) }
    }
    pub fn stream_index(&self) -> usize {
        unsafe { moonfire_ffmpeg_packet_stream_index(*self.0) as usize }
    }
    pub fn data(&self) -> Option<&[u8]> {
        unsafe {
            let d = moonfire_ffmpeg_packet_data(*self.0);
            if d.data.is_null() {
                None
            } else {
                Some(::std::slice::from_raw_parts(d.data, d.len))
            }
        }
    }
}

impl<'i> Drop for Packet<'i> {
    fn drop(&mut self) {
        unsafe {
            av_packet_unref(*self.0);
        }
    }
}

impl AVCodecParameters {
    pub fn extradata(&self) -> &[u8] {
        unsafe {
            let d = moonfire_ffmpeg_codecpar_extradata(self);
            ::std::slice::from_raw_parts(d.data, d.len)
        }
    }
    pub fn dims(&self) -> ImageDimensions {
        assert!(self.codec_type().is_video());
        unsafe { moonfire_ffmpeg_codecpar_dims(self) }
    }
    pub fn codec_id(&self) -> CodecId {
        unsafe { moonfire_ffmpeg_codecpar_codec_id(self) }
    }
    pub fn codec_type(&self) -> MediaType {
        unsafe { moonfire_ffmpeg_codecpar_codec_type(self) }
    }
}

pub struct InputCodecParameters<'s>(pub(crate) &'s AVCodecParameters);

impl<'s> InputCodecParameters<'s> {
    pub fn new_decoder(&self, options: &mut Dictionary) -> Result<DecodeContext, Error> {
        let decoder = match self.codec_id().find_decoder() {
            Some(d) => d,
            None => {
                return Err(Error::decoder_not_found());
            }
        };
        let mut c = decoder.alloc_context()?;
        Error::wrap(unsafe { avcodec_parameters_to_context(c.ctx.as_ptr(), self.0) })?;
        c.open(options)?;
        Ok(c)
    }
}

impl<'s> std::ops::Deref for InputCodecParameters<'s> {
    type Target = AVCodecParameters;
    fn deref(&self) -> &AVCodecParameters {
        self.0
    }
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct CodecId(libc::c_int);

impl CodecId {
    pub fn is_aac(self) -> bool {
        self.0 == unsafe { moonfire_ffmpeg_av_codec_id_aac }
    }

    pub fn is_h264(self) -> bool {
        self.0 == unsafe { moonfire_ffmpeg_av_codec_id_h264 }
    }

    pub fn find_decoder(self) -> Option<Decoder> {
        // avcodec_find_decoder returns an AVCodec which lives forever.
        unsafe { avcodec_find_decoder(self.0).as_ref() }.map(|d| Decoder(d))
    }

    pub fn find_encoder(self) -> Option<Encoder> {
        // avcodec_find_encoder returns an AVCodec which lives forever.
        unsafe { avcodec_find_encoder(self.0).as_ref() }.map(|e| Encoder(e))
    }
}

impl std::fmt::Debug for CodecId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "CodecId({} /* {} */)", self.0, self)
    }
}

impl std::fmt::Display for CodecId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = unsafe { std::ffi::CStr::from_ptr(avcodec_get_name(self.0)) };
        let s = s.to_str().map_err(|_| std::fmt::Error)?;
        std::fmt::Display::fmt(s, f)
    }
}

#[derive(Copy, Clone)]
pub struct Decoder(&'static AVCodec);

impl Decoder {
    fn alloc_context(self) -> Result<DecodeContext, Error> {
        let ctx = ptr::NonNull::new(unsafe { avcodec_alloc_context3(self.0) })
            .ok_or_else(Error::enomem)?;
        Ok(DecodeContext { decoder: self, ctx })
    }
}

pub struct DecodeContext {
    decoder: Decoder,
    ctx: ptr::NonNull<AVCodecContext>,
}

impl Drop for DecodeContext {
    fn drop(&mut self) {
        let mut ctx = self.ctx.as_ptr();
        unsafe { avcodec_free_context(&mut ctx) }
    }
}

impl DecodeContext {
    fn open(&mut self, options: &mut Dictionary) -> Result<(), Error> {
        Error::wrap(unsafe { avcodec_open2(self.ctx.as_mut(), self.decoder.0, options) })?;
        Ok(())
    }

    pub fn ctx(&self) -> &AVCodecContext {
        unsafe { self.ctx.as_ref() }
    }

    pub fn decode_video(&self, pkt: &Packet, frame: &mut VideoFrame) -> Result<bool, Error> {
        let mut got_picture: libc::c_int = 0;
        Error::wrap(unsafe {
            avcodec_decode_video2(
                self.ctx.as_ptr(),
                frame.frame.as_mut(),
                &mut got_picture,
                *pkt.0,
            )
        })?;
        if got_picture != 0 {
            unsafe { moonfire_ffmpeg_frame_stuff(frame.frame.as_ptr(), &mut frame.stuff) };
            return Ok(true);
        };
        Ok(false)
    }
}

#[derive(Copy, Clone)]
pub struct Encoder(&'static AVCodec);

impl Encoder {
    /*pub fn alloc_context(self) -> Result<EncodeContext, Error> {
        let ctx = unsafe { avcodec_alloc_context3(self.0) };
        if ctx.is_null() {
            return Err(Error::enomem());
        }
        Ok(EncodeContext {
            encoder: self,
            ctx,
        })
    }*/
}

pub struct EncodeContext<'a>(&'a mut AVCodecContext);

/*impl Drop for EncodeContext {
    fn drop(&mut self) {
        unsafe { avcodec_free_context(&mut self.ctx) }
    }
}*/

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct VideoParameters {
    width: libc::c_int,
    height: libc::c_int,
    sample_aspect_ratio: Rational,
    pix_fmt: PixelFormat,
    time_base: Rational,
}

impl<'a> EncodeContext<'a> {
    pub fn set_params(&mut self, p: &VideoParameters) {
        unsafe { moonfire_ffmpeg_cctx_set_params(self.0, p) };
    }

    pub fn open(&mut self, encoder: Encoder, options: &mut Dictionary) -> Result<(), Error> {
        Error::wrap(unsafe { avcodec_open2(self.0, encoder.0, options) })?;
        Ok(())
    }
}
