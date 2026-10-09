// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK's DMA-BUF renderer draws garbage on some GPUs (notably NVIDIA
    // on Wayland). Disable it unless the user decided otherwise.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: called before any other thread is started.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
    // With gst-plugins-bad installed, GStreamer prefers NVIDIA's hardware
    // decoders, and handing their output to WebKit takes the page down on
    // NVIDIA under Wayland. Software decoding is fast enough for these videos.
    #[cfg(target_os = "linux")]
    if std::env::var_os("GST_PLUGIN_FEATURE_RANK").is_none() {
        // SAFETY: called before any other thread is started.
        unsafe { std::env::set_var("GST_PLUGIN_FEATURE_RANK", NVDEC_OFF) };
    }
    pergament_app::run()
}

/// Rank 0 keeps GStreamer from choosing an element (`nvh264dec` and friends).
#[cfg(target_os = "linux")]
const NVDEC_OFF: &str = "nvh264dec:0,nvh265dec:0,nvav1dec:0,nvmpeg2videodec:0,nvmpeg4videodec:0,\
    nvmpegvideodec:0,nvvp8dec:0,nvvp9dec:0,nvjpegdec:0,nvh264sldec:0,nvh265sldec:0,\
    nvav1sldec:0,nvvp8sldec:0,nvvp9sldec:0";
