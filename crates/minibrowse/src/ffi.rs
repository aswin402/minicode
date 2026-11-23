//! Zero-`build.rs` Pure-Rust Dynamic FFI Loader (`dlopen` / `dlsym`) for WebKit2GTK, JSC, GTK3, Cairo & GLib.
//!
//! Compiles on 100% pure Rust without `-dev` C headers or `pkg-config`, dynamically probing
//! `4.1` -> `4.0` shared objects at runtime and degrading cleanly on headless servers without X11/Wayland.

use std::ffi::{c_char, c_int, c_long, c_ulong, c_void, CString};

#[repr(C)]
pub struct GError {
    pub domain: u32,
    pub code: c_int,
    pub message: *const c_char,
}

pub type GSourceFunc = unsafe extern "C" fn(*mut c_void) -> c_int;
pub type GDestroyNotify = unsafe extern "C" fn(*mut c_void);
pub type GAsyncReadyCallback =
    unsafe extern "C" fn(source_object: *mut c_void, res: *mut c_void, user_data: *mut c_void);
pub type CairoWriteFunc =
    unsafe extern "C" fn(closure: *mut c_void, data: *const u8, length: u32) -> c_int;

pub const G_PRIORITY_DEFAULT_IDLE: c_int = 200;
pub const GTK_WINDOW_TOPLEVEL: c_int = 0;
pub const GTK_ALIGN_FILL: c_int = 0;
pub const GTK_ALIGN_CENTER: c_int = 3;
pub const CAIRO_FORMAT_ARGB32: c_int = 0;
pub const CAIRO_STATUS_SUCCESS: c_int = 0;
pub const WEBKIT_CACHE_MODEL_DOCUMENT_VIEWER: c_int = 0;
pub const WEBKIT_USER_CONTENT_INJECTED_ALL_FRAMES: c_int = 0;
pub const WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START: c_int = 0;
pub const WEBKIT_HARDWARE_ACCELERATION_POLICY_ON_DEMAND: c_int = 0;
pub const WEBKIT_HARDWARE_ACCELERATION_POLICY_NEVER: c_int = 2;
pub const WEBKIT_SNAPSHOT_REGION_VISIBLE: c_int = 0;
pub const WEBKIT_SNAPSHOT_OPTIONS_NONE: c_int = 0;
pub const WEBKIT_LOAD_FINISHED: c_int = 3;
pub const WEBKIT_NETWORK_PROXY_MODE_CUSTOM: c_int = 2;
pub const GDK_WINDOW_STATE_WITHDRAWN: u32 = 1;
pub const GDK_WINDOW_STATE_ICONIFIED: u32 = 2;
pub const GDK_POINTER_MOTION_MASK: c_int = 1 << 2;
pub const GDK_BUTTON_PRESS_MASK: c_int = 1 << 8;
pub const GDK_KEY_PRESS_MASK: c_int = 1 << 10;
pub const GDK_SCROLL_MASK: c_int = 1 << 21;

#[cfg(target_os = "linux")]
extern "C" {
    fn malloc_trim(pad: usize) -> c_int;
}

/// Releases unused glibc heap pages back to the Linux kernel.
pub fn trim_process_heap() {
    #[cfg(target_os = "linux")]
    unsafe {
        let _ = malloc_trim(0);
    }
}

/// Applies adaptive low-memory allocator, JavaScriptCore heap bounds, GStreamer/decoder thread caps,
/// and Zero-EGL/Zero-LLVM 2D Cairo surface defaults (`~20–32 MB` headless; `~75–85 MB PSS` 2D GUI
/// instead of `~166 MB PSS` by preventing Mesa from loading `libgallium` + `libLLVM.so`).
pub fn tune_allocator_env(headless: bool) {
    let env_webgl = std::env::var("MINIBROWSE_ENABLE_WEBGL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    tune_allocator_env_with_webgl(headless, env_webgl);
}

/// Configures allocator, JSC, and EGL/LLVM environment variables with explicit `webgl` control.
///
/// Inspired by `Netzhaut`'s strict `nh-wsi` (OS window surface) vs `nh-gfx` (3D GPU stack) split:
/// standard 2D GUI windows bypass Mesa EGL vendor probing so neither the UI process nor
/// `WebKitWebProcess` `dlopen`s `libgallium` + `libLLVM.so` (~80 MB PSS saved).
pub fn tune_allocator_env_with_webgl(headless: bool, webgl: bool) {
    let common_vars: &[(&str, &str)] = &[
        ("MALLOC_ARENA_MAX", "2"),
        ("MALLOC_TRIM_THRESHOLD_", "65536"),
        ("G_SLICE", "always-malloc"),
        ("JavaScriptCoreUseBMalloc", "0"),
        ("JSC_useOMGJIT", "0"),
        ("JSC_numberOfDFGCompilerThreads", "1"),
        ("JSC_numberOfFTLCompilerThreads", "0"),
        ("JSC_numberOfWasmCompilerThreads", "1"),
        ("JSC_numberOfGCMarkers", "1"),
        ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        ("OMP_NUM_THREADS", "2"),
        ("GST_GL_WINDOW", "dummy"),
        ("GALLIUM_THREAD", "0"),
    ];
    for &(key, val) in common_vars {
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, val);
        }
    }

    if !webgl {
        for &(key, val) in &[
            ("GDK_GL", "disable"),
            ("GSK_RENDERER", "cairo"),
            ("WEBKIT_DISABLE_COMPOSITING_MODE", "1"),
        ] {
            if std::env::var_os(key).is_none() {
                std::env::set_var(key, val);
            }
        }
    }

    let respect_host_dpi = std::env::var("MINIBROWSE_RESPECT_HOST_DPI")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if headless || !respect_host_dpi {
        // Lock scale to 1.0 by default in both headless QA and GUI preview so host GNOME/Wayland
        // fractional text-scaling-factor (e.g. 1.1x) never shrinks CSS viewports (1440->1309).
        std::env::set_var("GDK_SCALE", "1");
        std::env::set_var("GDK_DPI_SCALE", "1");
    }

    if headless {
        for &(key, val) in &[
            ("JSC_forceRAMSize", "100663296"),
            ("JSC_useFTLJIT", "0"),
            ("JSC_useWasmFastMemory", "0"),
            ("LP_NUM_THREADS", "1"),
            ("LIBGL_ALWAYS_SOFTWARE", "1"),
        ] {
            if std::env::var_os(key).is_none() {
                std::env::set_var(key, val);
            }
        }
    } else {
        for &(key, val) in &[("JSC_forceRAMSize", "201326592"), ("LP_NUM_THREADS", "2")] {
            if std::env::var_os(key).is_none() {
                std::env::set_var(key, val);
            }
        }
    }
}

/// Updates process environment variables before spawning or recycling `WebKitWebProcess`
/// so child renderer processes dynamically enable or disable WebGL compositing on demand.
pub fn configure_runtime_webgl_env(webgl: bool) {
    if webgl {
        std::env::remove_var("WEBKIT_DISABLE_COMPOSITING_MODE");
        std::env::set_var("MINIBROWSE_ENABLE_WEBGL", "1");
    } else {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        std::env::remove_var("MINIBROWSE_ENABLE_WEBGL");
    }
}

/// Pure-Rust RFC-4648 standard Base64 encoder (zero external crate overhead).
#[must_use]
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(match chunk.len() {
            1 => '=',
            _ => ALPHABET[((n >> 6) & 0x3F) as usize] as char,
        });
        out.push(match chunk.len() {
            3 => ALPHABET[(n & 0x3F) as usize] as char,
            _ => '=',
        });
    }
    out
}

pub struct WebKitGtkApi {
    _handles: Vec<*mut c_void>,
    pub g_free: unsafe extern "C" fn(*mut c_void),
    pub g_error_free: unsafe extern "C" fn(*mut GError),
    pub g_object_ref: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    pub g_object_unref: unsafe extern "C" fn(*mut c_void),
    pub g_idle_add_full:
        unsafe extern "C" fn(c_int, GSourceFunc, *mut c_void, Option<GDestroyNotify>) -> u32,
    pub g_signal_connect_data: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        *const c_void,
        *mut c_void,
        *const c_void,
        u32,
    ) -> c_ulong,

    pub gtk_init_check: unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char) -> c_int,
    pub gtk_main: unsafe extern "C" fn(),
    pub gtk_main_quit: unsafe extern "C" fn(),
    pub gtk_main_iteration_do: unsafe extern "C" fn(c_int) -> c_int,
    pub gtk_window_new: unsafe extern "C" fn(c_int) -> *mut c_void,
    pub gtk_offscreen_window_new: unsafe extern "C" fn() -> *mut c_void,
    pub gtk_window_set_title: unsafe extern "C" fn(*mut c_void, *const c_char),
    pub gtk_window_set_default_size: unsafe extern "C" fn(*mut c_void, c_int, c_int),
    pub gtk_window_resize: unsafe extern "C" fn(*mut c_void, c_int, c_int),
    pub gtk_window_set_focus_on_map: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub gtk_window_set_accept_focus: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub gtk_window_present: Option<unsafe extern "C" fn(*mut c_void)>,
    pub gtk_window_set_default_icon: Option<unsafe extern "C" fn(*mut c_void)>,
    pub gtk_drawing_area_new: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub gtk_widget_add_events: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub gtk_widget_set_can_focus: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub gdk_pixbuf_new_from_data: Option<
        unsafe extern "C" fn(
            *const u8,
            c_int,
            c_int,
            c_int,
            c_int,
            c_int,
            c_int,
            *const c_void,
            *mut c_void,
        ) -> *mut c_void,
    >,
    pub gtk_container_add: unsafe extern "C" fn(*mut c_void, *mut c_void),
    pub gtk_container_remove: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    pub gtk_widget_set_size_request: unsafe extern "C" fn(*mut c_void, c_int, c_int),
    pub gtk_widget_set_halign: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub gtk_widget_set_valign: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub gtk_widget_show_all: unsafe extern "C" fn(*mut c_void),
    pub gtk_widget_hide: unsafe extern "C" fn(*mut c_void),
    pub gtk_widget_destroy: unsafe extern "C" fn(*mut c_void),
    pub gtk_widget_queue_draw: unsafe extern "C" fn(*mut c_void),
    pub gtk_widget_queue_resize: unsafe extern "C" fn(*mut c_void),
    pub gtk_widget_draw: unsafe extern "C" fn(*mut c_void, *mut c_void),
    pub gtk_widget_get_allocated_width: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub gtk_widget_get_allocated_height: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub gtk_widget_get_window: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    pub gdk_window_get_state: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    pub gtk_settings_get_default: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub gtk_settings_set_long_property:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, c_long, *const c_char)>,

    pub cairo_image_surface_create: unsafe extern "C" fn(c_int, c_int, c_int) -> *mut c_void,
    pub cairo_create: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub cairo_destroy: unsafe extern "C" fn(*mut c_void),
    pub cairo_surface_destroy: unsafe extern "C" fn(*mut c_void),
    pub cairo_surface_flush: unsafe extern "C" fn(*mut c_void),
    pub cairo_surface_write_to_png_stream:
        unsafe extern "C" fn(*mut c_void, CairoWriteFunc, *mut c_void) -> c_int,
    pub cairo_pdf_surface_create_for_stream:
        Option<unsafe extern "C" fn(CairoWriteFunc, *mut c_void, f64, f64) -> *mut c_void>,
    pub cairo_surface_finish: Option<unsafe extern "C" fn(*mut c_void)>,
    pub cairo_set_source_surface: Option<unsafe extern "C" fn(*mut c_void, *mut c_void, f64, f64)>,
    pub cairo_paint: Option<unsafe extern "C" fn(*mut c_void)>,
    pub cairo_scale: Option<unsafe extern "C" fn(*mut c_void, f64, f64)>,
    pub cairo_image_surface_get_width: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub cairo_image_surface_get_height: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub cairo_image_surface_get_stride: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub cairo_image_surface_get_data: Option<unsafe extern "C" fn(*mut c_void) -> *mut u8>,
    pub cairo_surface_mark_dirty: Option<unsafe extern "C" fn(*mut c_void)>,

    pub webkit_memory_pressure_settings_new: unsafe extern "C" fn() -> *mut c_void,
    pub webkit_memory_pressure_settings_set_memory_limit: unsafe extern "C" fn(*mut c_void, u32),
    pub webkit_memory_pressure_settings_set_conservative_threshold:
        unsafe extern "C" fn(*mut c_void, f64),
    pub webkit_memory_pressure_settings_set_strict_threshold:
        unsafe extern "C" fn(*mut c_void, f64),
    pub webkit_memory_pressure_settings_set_kill_threshold: unsafe extern "C" fn(*mut c_void, f64),
    pub webkit_memory_pressure_settings_set_poll_interval: unsafe extern "C" fn(*mut c_void, f64),
    pub webkit_memory_pressure_settings_free: unsafe extern "C" fn(*mut c_void),
    pub webkit_website_data_manager_set_memory_pressure_settings: unsafe extern "C" fn(*mut c_void),

    pub webkit_web_context_get_default: unsafe extern "C" fn() -> *mut c_void,
    pub webkit_web_context_set_cache_model: unsafe extern "C" fn(*mut c_void, c_int),
    pub webkit_web_context_set_web_process_count_limit: unsafe extern "C" fn(*mut c_void, u32),

    pub webkit_user_content_manager_new: unsafe extern "C" fn() -> *mut c_void,
    pub webkit_user_script_new: unsafe extern "C" fn(
        *const c_char,
        c_int,
        c_int,
        *const *const c_char,
        *const *const c_char,
    ) -> *mut c_void,
    pub webkit_user_script_unref: unsafe extern "C" fn(*mut c_void),
    pub webkit_user_content_manager_add_script: unsafe extern "C" fn(*mut c_void, *mut c_void),
    pub webkit_user_content_manager_remove_all_scripts: Option<unsafe extern "C" fn(*mut c_void)>,
    pub webkit_user_content_manager_register_script_message_handler:
        unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    pub g_bytes_new: Option<unsafe extern "C" fn(*const c_void, usize) -> *mut c_void>,
    pub g_bytes_unref: Option<unsafe extern "C" fn(*mut c_void)>,
    pub webkit_user_content_filter_store_new:
        Option<unsafe extern "C" fn(*const c_char) -> *mut c_void>,
    pub webkit_user_content_filter_store_save: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            *mut c_void,
            *mut c_void,
            Option<GAsyncReadyCallback>,
            *mut c_void,
        ),
    >,
    pub webkit_user_content_filter_store_save_finish:
        Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *mut *mut GError) -> *mut c_void>,
    pub webkit_user_content_manager_add_filter:
        Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    pub webkit_user_content_manager_remove_all_filters: Option<unsafe extern "C" fn(*mut c_void)>,
    pub webkit_user_content_filter_unref: Option<unsafe extern "C" fn(*mut c_void)>,

    pub webkit_web_view_new_with_user_content_manager:
        unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub webkit_web_view_get_settings: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub webkit_settings_set_enable_page_cache: unsafe extern "C" fn(*mut c_void, c_int),
    pub webkit_settings_set_enable_smooth_scrolling: unsafe extern "C" fn(*mut c_void, c_int),
    pub webkit_settings_set_hardware_acceleration_policy: unsafe extern "C" fn(*mut c_void, c_int),
    pub webkit_settings_set_auto_load_images: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub webkit_settings_set_enable_webgl: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub webkit_settings_set_enable_webaudio: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub webkit_settings_set_enable_media_stream: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub webkit_settings_set_enable_mediasource: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub webkit_settings_set_media_playback_requires_user_gesture:
        Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    pub webkit_settings_set_user_agent: Option<unsafe extern "C" fn(*mut c_void, *const c_char)>,
    pub webkit_web_view_is_playing_audio: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub webkit_web_view_set_zoom_level: Option<unsafe extern "C" fn(*mut c_void, f64)>,
    pub webkit_web_view_terminate_web_process: Option<unsafe extern "C" fn(*mut c_void)>,
    pub webkit_web_context_clear_cache: Option<unsafe extern "C" fn(*mut c_void)>,
    pub webkit_website_data_manager_new_ephemeral: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub webkit_network_proxy_settings_new:
        Option<unsafe extern "C" fn(*const c_char, *const *const c_char) -> *mut c_void>,
    pub webkit_network_proxy_settings_free: Option<unsafe extern "C" fn(*mut c_void)>,
    pub webkit_website_data_manager_set_network_proxy_settings:
        Option<unsafe extern "C" fn(*mut c_void, c_int, *mut c_void)>,
    pub webkit_web_context_new_with_website_data_manager:
        Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    pub webkit_web_view_get_type: Option<unsafe extern "C" fn() -> usize>,
    pub g_object_new: Option<unsafe extern "C" fn(usize, *const c_char, ...) -> *mut c_void>,
    pub webkit_web_view_load_uri: unsafe extern "C" fn(*mut c_void, *const c_char),
    pub webkit_web_view_load_html: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char),
    pub webkit_web_view_reload: unsafe extern "C" fn(*mut c_void),
    pub webkit_web_view_reload_bypass_cache: unsafe extern "C" fn(*mut c_void),
    pub webkit_web_view_can_go_back: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub webkit_web_view_can_go_forward: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub webkit_web_view_go_back: Option<unsafe extern "C" fn(*mut c_void)>,
    pub webkit_web_view_go_forward: Option<unsafe extern "C" fn(*mut c_void)>,
    pub webkit_web_view_get_uri: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    pub webkit_web_view_get_title: unsafe extern "C" fn(*mut c_void) -> *const c_char,

    pub webkit_web_view_evaluate_javascript: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        isize,
        *const c_char,
        *const c_char,
        *mut c_void,
        Option<GAsyncReadyCallback>,
        *mut c_void,
    ),
    pub webkit_web_view_evaluate_javascript_finish:
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut *mut GError) -> *mut c_void,
    pub webkit_web_view_get_snapshot: unsafe extern "C" fn(
        *mut c_void,
        c_int,
        c_int,
        *mut c_void,
        Option<GAsyncReadyCallback>,
        *mut c_void,
    ),
    pub webkit_web_view_get_snapshot_finish:
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut *mut GError) -> *mut c_void,
    pub webkit_javascript_result_get_js_value: unsafe extern "C" fn(*mut c_void) -> *mut c_void,

    pub jsc_value_is_undefined: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub jsc_value_is_null: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub jsc_value_is_string: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub jsc_value_to_string: unsafe extern "C" fn(*mut c_void) -> *mut c_char,
    pub jsc_value_to_json: unsafe extern "C" fn(*mut c_void, u32) -> *mut c_char,
}

// Safety: Function pointers resolved from process-global shared libraries are immutable code addresses.
unsafe impl Send for WebKitGtkApi {}
unsafe impl Sync for WebKitGtkApi {}

#[cfg(target_os = "linux")]
fn dlopen_any(candidates: &[&str]) -> Option<*mut c_void> {
    for &name in candidates {
        let Ok(cstr) = CString::new(name) else {
            continue;
        };
        // Safety: `cstr` is a NUL-terminated library soname.
        let handle = unsafe { libc::dlopen(cstr.as_ptr(), libc::RTLD_LAZY | libc::RTLD_GLOBAL) };
        if !handle.is_null() {
            return Some(handle);
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn dlopen_any(_candidates: &[&str]) -> Option<*mut c_void> {
    None
}

#[cfg(target_os = "linux")]
unsafe fn load_sym<T: Copy>(handles: &[*mut c_void], name: &str) -> Result<T, String> {
    let cstr = CString::new(name).map_err(|e| e.to_string())?;
    for &h in handles {
        let sym = libc::dlsym(h, cstr.as_ptr());
        if !sym.is_null() {
            return Ok(std::mem::transmute_copy::<*mut c_void, T>(&sym));
        }
    }
    Err(format!("Missing dynamic symbol: {name}"))
}

#[cfg(not(target_os = "linux"))]
unsafe fn load_sym<T: Copy>(_handles: &[*mut c_void], name: &str) -> Result<T, String> {
    let _ = CString::new(name);
    Err(format!(
        "WebKit2GTK dynamic loading is only supported on Linux (symbol: {name})"
    ))
}

unsafe fn load_opt_sym<T: Copy>(handles: &[*mut c_void], name: &str) -> Option<T> {
    load_sym(handles, name).ok()
}

impl WebKitGtkApi {
    /// Dynamically loads WebKit2GTK (`4.1` or `4.0`), JSC, GTK3, Cairo, GLib, and GObject at runtime.
    pub fn try_load() -> Result<Self, String> {
        let glib = dlopen_any(&["libglib-2.0.so.0", "libglib-2.0.so"])
            .ok_or_else(|| "libglib-2.0.so.0 not found".to_string())?;
        let gobj = dlopen_any(&["libgobject-2.0.so.0", "libgobject-2.0.so"])
            .ok_or_else(|| "libgobject-2.0.so.0 not found".to_string())?;
        let cairo = dlopen_any(&["libcairo.so.2", "libcairo.so"])
            .ok_or_else(|| "libcairo.so.2 not found".to_string())?;
        let gtk = dlopen_any(&["libgtk-3.so.0", "libgtk-3.so"])
            .ok_or_else(|| "libgtk-3.so.0 not found".to_string())?;
        let (wk, jsc) = match (
            dlopen_any(&["libwebkit2gtk-4.1.so.0", "libwebkit2gtk-4.1.so"]),
            dlopen_any(&[
                "libjavascriptcoregtk-4.1.so.0",
                "libjavascriptcoregtk-4.1.so",
            ]),
        ) {
            (Some(w), Some(j)) => (w, j),
            _ => (
                dlopen_any(&["libwebkit2gtk-4.0.so.37", "libwebkit2gtk-4.0.so"])
                    .ok_or_else(|| "libwebkit2gtk-4.1/4.0 not found".to_string())?,
                dlopen_any(&[
                    "libjavascriptcoregtk-4.0.so.18",
                    "libjavascriptcoregtk-4.0.so",
                ])
                .ok_or_else(|| "libjavascriptcoregtk-4.1/4.0 not found".to_string())?,
            ),
        };

        let mut handles = vec![wk, jsc, gtk, cairo, gobj, glib];
        if let Some(pixbuf) = dlopen_any(&["libgdk_pixbuf-2.0.so.0", "libgdk_pixbuf-2.0.so"]) {
            handles.push(pixbuf);
        }
        unsafe {
            Ok(Self {
                g_free: load_sym(&handles, "g_free")?,
                g_error_free: load_sym(&handles, "g_error_free")?,
                g_object_ref: load_opt_sym(&handles, "g_object_ref"),
                g_object_unref: load_sym(&handles, "g_object_unref")?,
                g_idle_add_full: load_sym(&handles, "g_idle_add_full")?,
                g_signal_connect_data: load_sym(&handles, "g_signal_connect_data")?,
                gtk_init_check: load_sym(&handles, "gtk_init_check")?,
                gtk_main: load_sym(&handles, "gtk_main")?,
                gtk_main_quit: load_sym(&handles, "gtk_main_quit")?,
                gtk_main_iteration_do: load_sym(&handles, "gtk_main_iteration_do")?,
                gtk_window_new: load_sym(&handles, "gtk_window_new")?,
                gtk_offscreen_window_new: load_sym(&handles, "gtk_offscreen_window_new")?,
                gtk_window_set_title: load_sym(&handles, "gtk_window_set_title")?,
                gtk_window_set_default_size: load_sym(&handles, "gtk_window_set_default_size")?,
                gtk_window_resize: load_sym(&handles, "gtk_window_resize")?,
                gtk_window_set_focus_on_map: load_opt_sym(&handles, "gtk_window_set_focus_on_map"),
                gtk_window_set_accept_focus: load_opt_sym(&handles, "gtk_window_set_accept_focus"),
                gtk_window_present: load_opt_sym(&handles, "gtk_window_present"),
                gtk_window_set_default_icon: load_opt_sym(&handles, "gtk_window_set_default_icon"),
                gtk_drawing_area_new: load_opt_sym(&handles, "gtk_drawing_area_new"),
                gtk_widget_add_events: load_opt_sym(&handles, "gtk_widget_add_events"),
                gtk_widget_set_can_focus: load_opt_sym(&handles, "gtk_widget_set_can_focus"),
                gdk_pixbuf_new_from_data: load_opt_sym(&handles, "gdk_pixbuf_new_from_data"),
                gtk_container_add: load_sym(&handles, "gtk_container_add")?,
                gtk_container_remove: load_opt_sym(&handles, "gtk_container_remove"),
                gtk_widget_set_size_request: load_sym(&handles, "gtk_widget_set_size_request")?,
                gtk_widget_set_halign: load_opt_sym(&handles, "gtk_widget_set_halign"),
                gtk_widget_set_valign: load_opt_sym(&handles, "gtk_widget_set_valign"),
                gtk_widget_show_all: load_sym(&handles, "gtk_widget_show_all")?,
                gtk_widget_hide: load_sym(&handles, "gtk_widget_hide")?,
                gtk_widget_destroy: load_sym(&handles, "gtk_widget_destroy")?,
                gtk_widget_queue_draw: load_sym(&handles, "gtk_widget_queue_draw")?,
                gtk_widget_queue_resize: load_sym(&handles, "gtk_widget_queue_resize")?,
                gtk_widget_draw: load_sym(&handles, "gtk_widget_draw")?,
                gtk_widget_get_allocated_width: load_sym(
                    &handles,
                    "gtk_widget_get_allocated_width",
                )?,
                gtk_widget_get_allocated_height: load_sym(
                    &handles,
                    "gtk_widget_get_allocated_height",
                )?,
                gtk_widget_get_window: load_opt_sym(&handles, "gtk_widget_get_window"),
                gdk_window_get_state: load_opt_sym(&handles, "gdk_window_get_state"),
                gtk_settings_get_default: load_opt_sym(&handles, "gtk_settings_get_default"),
                gtk_settings_set_long_property: load_opt_sym(
                    &handles,
                    "gtk_settings_set_long_property",
                ),
                cairo_image_surface_create: load_sym(&handles, "cairo_image_surface_create")?,
                cairo_create: load_sym(&handles, "cairo_create")?,
                cairo_destroy: load_sym(&handles, "cairo_destroy")?,
                cairo_surface_destroy: load_sym(&handles, "cairo_surface_destroy")?,
                cairo_surface_flush: load_sym(&handles, "cairo_surface_flush")?,
                cairo_surface_write_to_png_stream: load_sym(
                    &handles,
                    "cairo_surface_write_to_png_stream",
                )?,
                cairo_pdf_surface_create_for_stream: load_opt_sym(
                    &handles,
                    "cairo_pdf_surface_create_for_stream",
                ),
                cairo_surface_finish: load_opt_sym(&handles, "cairo_surface_finish"),
                cairo_set_source_surface: load_opt_sym(&handles, "cairo_set_source_surface"),
                cairo_paint: load_opt_sym(&handles, "cairo_paint"),
                cairo_scale: load_opt_sym(&handles, "cairo_scale"),
                cairo_image_surface_get_width: load_opt_sym(
                    &handles,
                    "cairo_image_surface_get_width",
                ),
                cairo_image_surface_get_height: load_opt_sym(
                    &handles,
                    "cairo_image_surface_get_height",
                ),
                cairo_image_surface_get_stride: load_opt_sym(
                    &handles,
                    "cairo_image_surface_get_stride",
                ),
                cairo_image_surface_get_data: load_opt_sym(
                    &handles,
                    "cairo_image_surface_get_data",
                ),
                cairo_surface_mark_dirty: load_opt_sym(&handles, "cairo_surface_mark_dirty"),
                webkit_memory_pressure_settings_new: load_sym(
                    &handles,
                    "webkit_memory_pressure_settings_new",
                )?,
                webkit_memory_pressure_settings_set_memory_limit: load_sym(
                    &handles,
                    "webkit_memory_pressure_settings_set_memory_limit",
                )?,
                webkit_memory_pressure_settings_set_conservative_threshold: load_sym(
                    &handles,
                    "webkit_memory_pressure_settings_set_conservative_threshold",
                )?,
                webkit_memory_pressure_settings_set_strict_threshold: load_sym(
                    &handles,
                    "webkit_memory_pressure_settings_set_strict_threshold",
                )?,
                webkit_memory_pressure_settings_set_kill_threshold: load_sym(
                    &handles,
                    "webkit_memory_pressure_settings_set_kill_threshold",
                )?,
                webkit_memory_pressure_settings_set_poll_interval: load_sym(
                    &handles,
                    "webkit_memory_pressure_settings_set_poll_interval",
                )?,
                webkit_memory_pressure_settings_free: load_sym(
                    &handles,
                    "webkit_memory_pressure_settings_free",
                )?,
                webkit_website_data_manager_set_memory_pressure_settings: load_sym(
                    &handles,
                    "webkit_website_data_manager_set_memory_pressure_settings",
                )?,
                webkit_web_context_get_default: load_sym(
                    &handles,
                    "webkit_web_context_get_default",
                )?,
                webkit_web_context_set_cache_model: load_sym(
                    &handles,
                    "webkit_web_context_set_cache_model",
                )?,
                webkit_web_context_set_web_process_count_limit: load_sym(
                    &handles,
                    "webkit_web_context_set_web_process_count_limit",
                )?,
                webkit_user_content_manager_new: load_sym(
                    &handles,
                    "webkit_user_content_manager_new",
                )?,
                webkit_user_script_new: load_sym(&handles, "webkit_user_script_new")?,
                webkit_user_script_unref: load_sym(&handles, "webkit_user_script_unref")?,
                webkit_user_content_manager_add_script: load_sym(
                    &handles,
                    "webkit_user_content_manager_add_script",
                )?,
                webkit_user_content_manager_remove_all_scripts: load_opt_sym(
                    &handles,
                    "webkit_user_content_manager_remove_all_scripts",
                ),
                webkit_user_content_manager_register_script_message_handler: load_sym(
                    &handles,
                    "webkit_user_content_manager_register_script_message_handler",
                )?,
                g_bytes_new: load_opt_sym(&handles, "g_bytes_new"),
                g_bytes_unref: load_opt_sym(&handles, "g_bytes_unref"),
                webkit_user_content_filter_store_new: load_opt_sym(
                    &handles,
                    "webkit_user_content_filter_store_new",
                ),
                webkit_user_content_filter_store_save: load_opt_sym(
                    &handles,
                    "webkit_user_content_filter_store_save",
                ),
                webkit_user_content_filter_store_save_finish: load_opt_sym(
                    &handles,
                    "webkit_user_content_filter_store_save_finish",
                ),
                webkit_user_content_manager_add_filter: load_opt_sym(
                    &handles,
                    "webkit_user_content_manager_add_filter",
                ),
                webkit_user_content_manager_remove_all_filters: load_opt_sym(
                    &handles,
                    "webkit_user_content_manager_remove_all_filters",
                ),
                webkit_user_content_filter_unref: load_opt_sym(
                    &handles,
                    "webkit_user_content_filter_unref",
                ),
                webkit_web_view_new_with_user_content_manager: load_sym(
                    &handles,
                    "webkit_web_view_new_with_user_content_manager",
                )?,
                webkit_web_view_get_settings: load_sym(&handles, "webkit_web_view_get_settings")?,
                webkit_settings_set_enable_page_cache: load_sym(
                    &handles,
                    "webkit_settings_set_enable_page_cache",
                )?,
                webkit_settings_set_enable_smooth_scrolling: load_sym(
                    &handles,
                    "webkit_settings_set_enable_smooth_scrolling",
                )?,
                webkit_settings_set_hardware_acceleration_policy: load_sym(
                    &handles,
                    "webkit_settings_set_hardware_acceleration_policy",
                )?,
                webkit_settings_set_auto_load_images: load_opt_sym(
                    &handles,
                    "webkit_settings_set_auto_load_images",
                ),
                webkit_settings_set_enable_webgl: load_opt_sym(
                    &handles,
                    "webkit_settings_set_enable_webgl",
                ),
                webkit_settings_set_enable_webaudio: load_opt_sym(
                    &handles,
                    "webkit_settings_set_enable_webaudio",
                ),
                webkit_settings_set_enable_media_stream: load_opt_sym(
                    &handles,
                    "webkit_settings_set_enable_media_stream",
                ),
                webkit_settings_set_enable_mediasource: load_opt_sym(
                    &handles,
                    "webkit_settings_set_enable_mediasource",
                ),
                webkit_settings_set_media_playback_requires_user_gesture: load_opt_sym(
                    &handles,
                    "webkit_settings_set_media_playback_requires_user_gesture",
                ),
                webkit_settings_set_user_agent: load_opt_sym(
                    &handles,
                    "webkit_settings_set_user_agent",
                ),
                webkit_web_view_is_playing_audio: load_opt_sym(
                    &handles,
                    "webkit_web_view_is_playing_audio",
                ),
                webkit_web_view_set_zoom_level: load_opt_sym(
                    &handles,
                    "webkit_web_view_set_zoom_level",
                ),
                webkit_web_view_terminate_web_process: load_opt_sym(
                    &handles,
                    "webkit_web_view_terminate_web_process",
                ),
                webkit_web_context_clear_cache: load_opt_sym(
                    &handles,
                    "webkit_web_context_clear_cache",
                ),
                webkit_website_data_manager_new_ephemeral: load_opt_sym(
                    &handles,
                    "webkit_website_data_manager_new_ephemeral",
                ),
                webkit_network_proxy_settings_new: load_opt_sym(
                    &handles,
                    "webkit_network_proxy_settings_new",
                ),
                webkit_network_proxy_settings_free: load_opt_sym(
                    &handles,
                    "webkit_network_proxy_settings_free",
                ),
                webkit_website_data_manager_set_network_proxy_settings: load_opt_sym(
                    &handles,
                    "webkit_website_data_manager_set_network_proxy_settings",
                ),
                webkit_web_context_new_with_website_data_manager: load_opt_sym(
                    &handles,
                    "webkit_web_context_new_with_website_data_manager",
                ),
                webkit_web_view_get_type: load_opt_sym(&handles, "webkit_web_view_get_type"),
                g_object_new: load_opt_sym(&handles, "g_object_new"),
                webkit_web_view_load_uri: load_sym(&handles, "webkit_web_view_load_uri")?,
                webkit_web_view_load_html: load_sym(&handles, "webkit_web_view_load_html")?,
                webkit_web_view_reload: load_sym(&handles, "webkit_web_view_reload")?,
                webkit_web_view_reload_bypass_cache: load_sym(
                    &handles,
                    "webkit_web_view_reload_bypass_cache",
                )?,
                webkit_web_view_can_go_back: load_opt_sym(&handles, "webkit_web_view_can_go_back"),
                webkit_web_view_can_go_forward: load_opt_sym(
                    &handles,
                    "webkit_web_view_can_go_forward",
                ),
                webkit_web_view_go_back: load_opt_sym(&handles, "webkit_web_view_go_back"),
                webkit_web_view_go_forward: load_opt_sym(&handles, "webkit_web_view_go_forward"),
                webkit_web_view_get_uri: load_sym(&handles, "webkit_web_view_get_uri")?,
                webkit_web_view_get_title: load_sym(&handles, "webkit_web_view_get_title")?,
                webkit_web_view_evaluate_javascript: load_sym(
                    &handles,
                    "webkit_web_view_evaluate_javascript",
                )?,
                webkit_web_view_evaluate_javascript_finish: load_sym(
                    &handles,
                    "webkit_web_view_evaluate_javascript_finish",
                )?,
                webkit_web_view_get_snapshot: load_sym(&handles, "webkit_web_view_get_snapshot")?,
                webkit_web_view_get_snapshot_finish: load_sym(
                    &handles,
                    "webkit_web_view_get_snapshot_finish",
                )?,
                webkit_javascript_result_get_js_value: load_sym(
                    &handles,
                    "webkit_javascript_result_get_js_value",
                )?,
                jsc_value_is_undefined: load_sym(&handles, "jsc_value_is_undefined")?,
                jsc_value_is_null: load_sym(&handles, "jsc_value_is_null")?,
                jsc_value_is_string: load_sym(&handles, "jsc_value_is_string")?,
                jsc_value_to_string: load_sym(&handles, "jsc_value_to_string")?,
                jsc_value_to_json: load_sym(&handles, "jsc_value_to_json")?,
                _handles: handles,
            })
        }
    }
}
