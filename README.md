# moonfire-ffmpeg

**Maintenance status:** inactive. May or may not become active again.

A Rust wrapper around select parts of [ffmpeg](http://www.ffmpeg.org/) needed
for Moonfire NVR:

*   ~~basic streaming: connecting to a RTSP stream.~~
    Moonfire NVR now uses a pure-Rust RTSP library,
    [Retina](https://crates.io/crates/retina).
*   future video analytics: decoding H.264 (likely also H.265 eventually; and
    eventually with hardware acceleration), converting its colorspace to RGB,
    and downscaling it to feed to
    [moonfire-tflite](https://github.com/scottlamb/moonfire-tflite).

There's a much more full-featured [ffmpeg](https://crates.io/crates/ffmpeg)
crate. A few reasons I use my own instead though:

*   the ffmpeg crate isn't actively maintained. (There are some forks though.
    Maybe [ffmpeg4](https://crates.io/crates/ffmpeg4) is what you're looking
    for.)
*   building moonfire-ffmpeg doesn't need bindgen, which can be a pain to
    install on some platforms. (See its
    [requirements](https://rust-lang.github.io/rust-bindgen/requirements.html).)
    Instead, moonfire-ffmpeg uses a very thin C wrapper around ffmpeg to avoid
    baking in (possibly version-specific) details of the ffmpeg ABI such as
    struct layouts.
*   moonfire-ffmpeg checks ABI compatibility between the ffmpeg it was
    compiled with and the ffmpeg it's actually running with, which is
    important for making it fool-proof with shared libraries. I think the
    ffmpeg crate is more intended to be used with a static library.
*   moonfire-ffmpeg exposes a few very specific pieces of the ffmpeg API that
    I needed and the ffmpeg crate doesn't, such as reading non-understood
    parameters to `avcodec_open2`.
*   moonfire-ffmpeg is licensed under MIT and Apache-2.0, vs the ffmpeg
    crate's WTFPL. I consider this an advantage for moonfire-ffmpeg because
    WTFPL [scares away](https://opensource.google/docs/thirdparty/licenses/#wtfpl-not-allowed)
    some people and companies.
