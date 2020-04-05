// Copyright (C) 2017-2020 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::avutil::{Error, ImageDimensions, PixelFormat, VideoFrame};
use std::ptr;

//#[link(name = "swscale")]
extern "C" {
    pub(crate) fn swscale_version() -> libc::c_int;

    fn sws_getContext(
        src_w: libc::c_int,
        src_h: libc::c_int,
        src_fmt: PixelFormat,
        dst_w: libc::c_int,
        dst_h: libc::c_int,
        dst_fmt: PixelFormat,
        flags: libc::c_int,
        src_filter: *mut SwsFilter,
        dst_filter: *mut SwsFilter,
        param: *const libc::c_double,
    ) -> *mut SwsContext;
    fn sws_freeContext(ctx: *mut SwsContext);
    fn sws_scale(
        c: *mut SwsContext,
        src_slice: *const *const u8,
        src_stride: *const libc::c_int,
        src_slice_y: libc::c_int,
        src_slice_h: libc::c_int,
        dst_slice: *const *mut u8,
        dst_stride: *const libc::c_int,
    );
}

//#[link(name = "wrapper")]
extern "C" {
    pub(crate) static moonfire_ffmpeg_compiled_libswscale_version: libc::c_int;

    static moonfire_ffmpeg_sws_bilinear: libc::c_int;
}

#[repr(C)]
struct SwsContext {
    _private: [u8; 0],
}
#[repr(C)]
struct SwsFilter {
    _private: [u8; 0],
}

pub struct Scaler {
    ctx: ptr::NonNull<SwsContext>,
    src: ImageDimensions,
    dst: ImageDimensions,
}

impl Scaler {
    pub fn new(src: ImageDimensions, dst: ImageDimensions) -> Result<Self, Error> {
        // TODO: yuvj420p causes an annoying warning "deprecated pixel format used, make sure you
        // did set range correctly" here. Looks like we need to change to yuv420p and call
        // sws_setColorspaceDetails to get the same effect while suppressing this warning.
        let ctx = ptr::NonNull::new(unsafe {
            sws_getContext(
                src.width,
                src.height,
                src.pix_fmt,
                dst.width,
                dst.height,
                dst.pix_fmt,
                moonfire_ffmpeg_sws_bilinear,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        })
        .ok_or_else(Error::unknown)?;
        Ok(Scaler { ctx, src, dst })
    }

    pub fn scale(&mut self, src: &VideoFrame, dst: &mut VideoFrame) {
        assert_eq!(src.dims(), self.src);
        assert_eq!(dst.dims(), self.dst);
        unsafe {
            sws_scale(
                self.ctx.as_ptr(),
                src.stuff.data.cast(),
                src.stuff.linesizes,
                0,
                self.src.height,
                dst.stuff.data,
                dst.stuff.linesizes,
            )
        };
    }
}

impl Drop for Scaler {
    fn drop(&mut self) {
        unsafe { sws_freeContext(self.ctx.as_ptr()) }
    }
}
