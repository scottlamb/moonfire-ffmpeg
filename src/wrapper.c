// Copyright (C) 2021 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0
// vim: set sw=4 et:

#include <libavcodec/avcodec.h>
#include <libavcodec/version.h>
#include <libavformat/avformat.h>
#include <libavformat/version.h>
#include <libavutil/avutil.h>
#include <libavutil/dict.h>
#include <libavutil/imgutils.h>
#include <libavutil/log.h>
#include <libavutil/version.h>
#ifdef MOONFIRE_USE_SWSCALE
#include <libswscale/swscale.h>
#include <libswscale/version.h>
#endif
#include <assert.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdlib.h>
#include <stdio.h>

const int moonfire_ffmpeg_compiled_libavcodec_version = LIBAVCODEC_VERSION_INT;
const int moonfire_ffmpeg_compiled_libavformat_version = LIBAVFORMAT_VERSION_INT;
const int moonfire_ffmpeg_compiled_libavutil_version = LIBAVUTIL_VERSION_INT;

#ifdef MOONFIRE_USE_SWSCALE
const int moonfire_ffmpeg_compiled_libswscale_version = LIBSWSCALE_VERSION_INT;
const int moonfire_ffmpeg_sws_bilinear = SWS_BILINEAR;
#endif

const int moonfire_ffmpeg_av_dict_ignore_suffix = AV_DICT_IGNORE_SUFFIX;

const int64_t moonfire_ffmpeg_av_nopts_value = AV_NOPTS_VALUE;

const int moonfire_ffmpeg_avmedia_type_audio = AVMEDIA_TYPE_AUDIO;
const int moonfire_ffmpeg_avmedia_type_data = AVMEDIA_TYPE_DATA;
const int moonfire_ffmpeg_avmedia_type_video = AVMEDIA_TYPE_VIDEO;

const int moonfire_ffmpeg_av_codec_id_aac = AV_CODEC_ID_AAC;
const int moonfire_ffmpeg_av_codec_id_h264 = AV_CODEC_ID_H264;

const int moonfire_ffmpeg_averror_decoder_not_found = AVERROR_DECODER_NOT_FOUND;
const int moonfire_ffmpeg_averror_invalid_data = AVERROR_INVALIDDATA;
const int moonfire_ffmpeg_averror_eof = AVERROR_EOF;
const int moonfire_ffmpeg_averror_enomem = AVERROR(ENOMEM);
const int moonfire_ffmpeg_averror_enosys = AVERROR(ENOSYS);
const int moonfire_ffmpeg_averror_unknown = AVERROR_UNKNOWN;

const int moonfire_ffmpeg_pix_fmt_rgb24 = AV_PIX_FMT_RGB24;
const int moonfire_ffmpeg_pix_fmt_bgr24 = AV_PIX_FMT_BGR24;

const int moonfire_ffmpeg_avseek_force = AVSEEK_FORCE;
const int moonfire_ffmpeg_avseek_size = AVSEEK_SIZE;
const int moonfire_ffmpeg_seek_set = SEEK_SET;
const int moonfire_ffmpeg_seek_cur = SEEK_CUR;
const int moonfire_ffmpeg_seek_end = SEEK_END;

typedef void (*RustLogCallback)(
    const char *avc_item_name,
    void *avc,
    int level,
    const char *fmt,
    void *vl);

static RustLogCallback rust_log_callback;

// Prior to libavcodec 58.9.100, multithreaded callers were expected to supply
// a lock callback. That release deprecated this API. It also introduced a
// FF_API_LOCKMGR #define to track its removal:
//
// * older builds (in which the lock callback is needed) don't define it.
// * middle builds (in which the callback is deprecated) define it as 1.
//   value of 1.
// * future builds (in which the callback removed) will define
//   it as 0.
//
// so (counterintuitively) use the lock manager when FF_API_LOCKMGR is
// undefined.

#ifndef FF_API_LOCKMGR
static int lock_callback(void **mutex, enum AVLockOp op) {
    switch (op) {
        case AV_LOCK_CREATE:
            *mutex = malloc(sizeof(pthread_mutex_t));
            if (*mutex == NULL)
                return -1;
            if (pthread_mutex_init(*mutex, NULL) != 0)
                return -1;
            break;
        case AV_LOCK_DESTROY:
            if (pthread_mutex_destroy(*mutex) != 0)
                return -1;
            free(*mutex);
            *mutex = NULL;
            break;
        case AV_LOCK_OBTAIN:
            if (pthread_mutex_lock(*mutex) != 0)
                return -1;
            break;
        case AV_LOCK_RELEASE:
            if (pthread_mutex_unlock(*mutex) != 0)
                return -1;
            break;
        default:
            return -1;
    }
    return 0;
}
#endif

// Wrap the va_list in a structure because va_list is (on some platforms)
// an array, and this covers up C's annoying habit of treating arrays as
// pointers even when they're supposedly opaque types.
struct my_va_list {
    va_list v;
};

static void log_callback(void *avcl, int level, const char *fmt, va_list vl) {
    // avcl is (according to av_log_default_callback's docstring) "a pointer
    // to an arbitrary struct of which the first field is a pointer to an
    // AVClass struct". The av_log_default_callback is defensive to both avcl
    // itself and the AVClass being NULL; match that.
    AVClass *avc = (avcl == NULL) ? NULL : *(AVClass **)avcl;
    const char *avc_item_name = (avc == NULL) ? NULL : avc->item_name(avcl);

    // av_log_default_callback also looks up a parent, but it looks like that's
    // rarely supplied. Skip it for now.

    struct my_va_list v;
    va_copy(v.v, vl);
    va_end(vl);
    rust_log_callback(
        avc_item_name,
        avcl,
        level,
        fmt,
        &v);
}

int moonfire_ffmpeg_vsnprintf(char *buf, size_t size, const char *fmt,
                              struct my_va_list *vl) {
    return vsnprintf(buf, size, fmt, vl->v);
}

void moonfire_ffmpeg_init(RustLogCallback cb) {
#ifndef FF_API_LOCKMGR
    if (av_lockmgr_register(&lock_callback) < 0) {
        abort();
    }
#endif
    rust_log_callback = cb;
    av_log_set_callback(&log_callback);
}

struct moonfire_ffmpeg_streams {
    AVStream** streams;
    size_t len;
};

struct moonfire_ffmpeg_data {
    uint8_t *data;
    size_t len;
};

struct VideoParameters {
    int width;
    int height;
    AVRational sample_aspect_ratio;
    enum AVPixelFormat pix_fmt;
    AVRational time_base;
};

struct moonfire_ffmpeg_image_dimensions {
    int width;
    int height;
    int pix_fmt;
};

struct moonfire_ffmpeg_frame_stuff {
    struct moonfire_ffmpeg_image_dimensions dims;
    uint8_t **data;
    int *linesizes;
    int64_t pts;
};

struct moonfire_ffmpeg_streams moonfire_ffmpeg_fctx_streams(AVFormatContext *ctx) {
    struct moonfire_ffmpeg_streams s = {ctx->streams, ctx->nb_streams};
    return s;
}

int moonfire_ffmpeg_fctx_open_write(AVFormatContext *ctx, const char *url) {
    return avio_open(&ctx->pb, url, AVIO_FLAG_WRITE);
}

void moonfire_ffmpeg_fctx_set_pb(AVFormatContext *ctx, AVIOContext *pb) {
    assert(ctx->pb == NULL);
    ctx->pb = pb;
}

void moonfire_ffmpeg_ioctx_set_direct(AVIOContext *pb) {
    pb->direct = 1;
}

void moonfire_ffmpeg_cctx_params(const AVCodecContext *ctx, struct VideoParameters *p) {
    p->width = ctx->width;
    p->height = ctx->height;
    p->sample_aspect_ratio = ctx->sample_aspect_ratio;
    p->pix_fmt = ctx->pix_fmt;
    p->time_base = ctx->time_base;
}

void moonfire_ffmpeg_cctx_set_params(AVCodecContext *ctx, const struct VideoParameters *p) {
    ctx->width = p->width;
    ctx->height = p->height;
    ctx->sample_aspect_ratio = p->sample_aspect_ratio;
    ctx->pix_fmt = p->pix_fmt;
    ctx->time_base = p->time_base;
}

AVPacket *moonfire_ffmpeg_packet_alloc(void) { return malloc(sizeof(AVPacket)); }
void moonfire_ffmpeg_packet_free(AVPacket *pkt) { free(pkt); }
bool moonfire_ffmpeg_packet_is_key(AVPacket *pkt) { return (pkt->flags & AV_PKT_FLAG_KEY) != 0; }
int64_t moonfire_ffmpeg_packet_pts(AVPacket *pkt) { return pkt->pts; }
void moonfire_ffmpeg_packet_set_dts(AVPacket *pkt, int64_t dts) { pkt->dts = dts; }
void moonfire_ffmpeg_packet_set_pts(AVPacket *pkt, int64_t pts) { pkt->pts = pts; }
void moonfire_ffmpeg_packet_set_duration(AVPacket *pkt, int dur) { pkt->duration = dur; }
int64_t moonfire_ffmpeg_packet_dts(AVPacket *pkt) { return pkt->dts; }
int moonfire_ffmpeg_packet_duration(AVPacket *pkt) { return pkt->duration; }
int moonfire_ffmpeg_packet_stream_index(AVPacket *pkt) { return pkt->stream_index; }
struct moonfire_ffmpeg_data moonfire_ffmpeg_packet_data(AVPacket *pkt) {
    struct moonfire_ffmpeg_data d = {pkt->data, pkt->size};
    return d;
}

AVCodecParameters *moonfire_ffmpeg_stream_codecpar(AVStream *stream) { return stream->codecpar; }
int64_t moonfire_ffmpeg_stream_duration(AVStream *stream) { return stream->duration; }
AVRational moonfire_ffmpeg_stream_time_base(AVStream *stream) { return stream->time_base; }

int moonfire_ffmpeg_cctx_codec_id(AVCodecContext *cctx) { return cctx->codec_id; }
int moonfire_ffmpeg_cctx_codec_type(AVCodecContext *cctx) { return cctx->codec_type; }
int moonfire_ffmpeg_cctx_height(AVCodecContext *cctx) { return cctx->height; }
int moonfire_ffmpeg_cctx_width(AVCodecContext *cctx) { return cctx->width; }
int moonfire_ffmpeg_cctx_pix_fmt(AVCodecContext *cctx) { return cctx->pix_fmt; }

int moonfire_ffmpeg_frame_image_alloc(
    AVFrame* frame, struct moonfire_ffmpeg_image_dimensions* dims) {
    // TODO: any reason to support an alignment other than 32?
    int r = av_image_alloc(frame->data, frame->linesize, dims->width, dims->height, dims->pix_fmt,
                           32);
    if (r < 0) {
        return r;
    }
    frame->width = dims->width;
    frame->height = dims->height;
    frame->format = dims->pix_fmt;
    return r;
}

void moonfire_ffmpeg_frame_stuff(AVFrame *frame,
                                 struct moonfire_ffmpeg_frame_stuff* s) {
    s->dims.width = frame->width;
    s->dims.height = frame->height;
    s->dims.pix_fmt = frame->format;
    s->data = frame->data;
    s->linesizes = frame->linesize;
    s->pts = frame->pts;
}

int moonfire_ffmpeg_codecpar_codec_id(AVCodecParameters *codecpar) { return codecpar->codec_id; }
int moonfire_ffmpeg_codecpar_codec_type(AVCodecParameters *codecpar) {
    return codecpar->codec_type;
}
struct moonfire_ffmpeg_image_dimensions moonfire_ffmpeg_codecpar_dims(AVCodecParameters *codecpar) {
    struct moonfire_ffmpeg_image_dimensions d = {
        .width = codecpar->width,
        .height = codecpar->height,
        .pix_fmt = codecpar->format
    };
    return d;
}
struct moonfire_ffmpeg_data moonfire_ffmpeg_codecpar_extradata(AVCodecParameters *codecpar) {
    struct moonfire_ffmpeg_data d = {codecpar->extradata, codecpar->extradata_size};
    return d;
}
