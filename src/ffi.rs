#![allow(dead_code)]

use std::ffi::{c_char, c_int, c_long, c_uint, c_void};

pub type W = *mut c_void;
pub type Callback = Option<unsafe extern "C" fn()>;

#[link(name = "gtk-3")]
unsafe extern "C" {
    pub fn gtk_init_check(argc: *mut c_int, argv: *mut *mut *mut c_char) -> c_int;
    pub fn gtk_main();
    pub fn gtk_main_quit();
    pub fn gtk_settings_get_default() -> W;

    pub fn gtk_window_new(kind: c_int) -> W;
    pub fn gtk_window_set_title(window: W, title: *const c_char);
    pub fn gtk_window_set_default_size(window: W, width: c_int, height: c_int);
    pub fn gtk_window_set_position(window: W, position: c_int);
    pub fn gtk_window_set_icon_name(window: W, name: *const c_char);
    pub fn gtk_window_set_icon(window: W, pixbuf: W);
    pub fn gtk_window_set_default_icon(pixbuf: W);
    pub fn gtk_window_present(window: W);
    pub fn gtk_window_set_transient_for(window: W, parent: W);
    pub fn gtk_window_set_modal(window: W, modal: c_int);
    pub fn gtk_window_set_destroy_with_parent(window: W, destroy: c_int);
    pub fn gtk_window_set_resizable(window: W, resizable: c_int);

    pub fn gtk_widget_show_all(widget: W);
    pub fn gtk_widget_hide(widget: W);
    pub fn gtk_widget_show(widget: W);
    pub fn gtk_widget_destroy(widget: W);
    pub fn gtk_widget_set_name(widget: W, name: *const c_char);
    pub fn gtk_widget_set_sensitive(widget: W, sensitive: c_int);
    pub fn gtk_widget_set_visible(widget: W, visible: c_int);
    pub fn gtk_widget_set_size_request(widget: W, width: c_int, height: c_int);
    pub fn gtk_widget_set_hexpand(widget: W, expand: c_int);
    pub fn gtk_widget_set_vexpand(widget: W, expand: c_int);
    pub fn gtk_widget_set_halign(widget: W, align: c_int);
    pub fn gtk_widget_set_valign(widget: W, align: c_int);
    pub fn gtk_widget_set_margin_start(widget: W, margin: c_int);
    pub fn gtk_widget_set_margin_end(widget: W, margin: c_int);
    pub fn gtk_widget_set_margin_top(widget: W, margin: c_int);
    pub fn gtk_widget_set_margin_bottom(widget: W, margin: c_int);
    pub fn gtk_widget_get_style_context(widget: W) -> W;
    pub fn gtk_style_context_add_class(context: W, class: *const c_char);
    pub fn gtk_style_context_remove_class(context: W, class: *const c_char);
    pub fn gtk_widget_grab_focus(widget: W);
    pub fn gtk_widget_set_focus_on_click(widget: W, focus: c_int);
    pub fn gtk_widget_set_no_show_all(widget: W, no_show: c_int);
    pub fn gtk_widget_get_allocated_width(widget: W) -> c_int;
    pub fn gtk_widget_get_allocated_height(widget: W) -> c_int;
    pub fn gtk_widget_add_events(widget: W, events: c_int);
    pub fn gtk_widget_draw(widget: W, cairo: W);
    pub fn gtk_widget_queue_draw(widget: W);

    pub fn gtk_box_new(orientation: c_int, spacing: c_int) -> W;
    pub fn gtk_box_pack_start(container: W, child: W, expand: c_int, fill: c_int, padding: c_uint);
    pub fn gtk_box_set_spacing(container: W, spacing: c_int);
    pub fn gtk_box_set_homogeneous(container: W, homogeneous: c_int);
    pub fn gtk_orientable_set_orientation(orientable: W, orientation: c_int);
    pub fn gtk_container_add(container: W, child: W);
    pub fn gtk_container_set_border_width(container: W, width: c_uint);
    pub fn gtk_bin_get_child(bin: W) -> W;

    pub fn gtk_label_new(text: *const c_char) -> W;
    pub fn gtk_label_set_text(label: W, text: *const c_char);
    pub fn gtk_label_set_markup(label: W, text: *const c_char);
    pub fn gtk_label_set_xalign(label: W, align: f32);
    pub fn gtk_label_set_line_wrap(label: W, wrap: c_int);
    pub fn gtk_label_set_max_width_chars(label: W, width: c_int);
    pub fn gtk_label_set_ellipsize(label: W, mode: c_int);
    pub fn gtk_label_set_justify(label: W, justify: c_int);
    pub fn gtk_label_set_selectable(label: W, selectable: c_int);

    pub fn gtk_image_new_from_pixbuf(pixbuf: W) -> W;
    pub fn gtk_image_set_from_pixbuf(image: W, pixbuf: W);
    pub fn gtk_image_new_from_icon_name(name: *const c_char, size: c_int) -> W;

    pub fn gtk_button_new() -> W;
    pub fn gtk_button_new_with_label(label: *const c_char) -> W;
    pub fn gtk_button_new_from_icon_name(name: *const c_char, size: c_int) -> W;
    pub fn gtk_button_set_label(button: W, label: *const c_char);
    pub fn gtk_button_set_image(button: W, image: W);
    pub fn gtk_button_set_always_show_image(button: W, show: c_int);
    pub fn gtk_button_set_image_position(button: W, position: c_int);
    pub fn gtk_button_set_relief(button: W, relief: c_int);

    pub fn gtk_entry_new() -> W;
    pub fn gtk_entry_set_text(entry: W, text: *const c_char);
    pub fn gtk_editable_set_editable(editable: W, editable_flag: c_int);
    pub fn gtk_entry_get_text(entry: W) -> *const c_char;
    pub fn gtk_entry_set_activates_default(entry: W, activates: c_int);

    pub fn gtk_switch_new() -> W;
    pub fn gtk_switch_get_active(switch: W) -> c_int;
    pub fn gtk_switch_set_active(switch: W, active: c_int);

    pub fn gtk_toggle_button_new() -> W;
    pub fn gtk_toggle_button_get_active(button: W) -> c_int;
    pub fn gtk_toggle_button_set_active(button: W, active: c_int);
    pub fn gtk_drawing_area_new() -> W;

    pub fn gtk_combo_box_text_new() -> W;
    pub fn gtk_combo_box_text_append(combo: W, id: *const c_char, text: *const c_char);
    pub fn gtk_combo_box_text_remove_all(combo: W);
    pub fn gtk_combo_box_text_get_active_id(combo: W) -> *mut c_char;
    pub fn gtk_combo_box_set_active(combo: W, index: c_int);
    pub fn gtk_combo_box_get_active(combo: W) -> c_int;
    pub fn gtk_combo_box_set_active_id(combo: W, id: *const c_char) -> c_int;
    pub fn gtk_combo_box_get_active_id(combo: W) -> *mut c_char;

    pub fn gtk_notebook_new() -> W;
    pub fn gtk_notebook_append_page(notebook: W, child: W, label: W) -> c_int;
    pub fn gtk_notebook_set_current_page(notebook: W, page: c_int);
    pub fn gtk_notebook_set_show_tabs(notebook: W, show: c_int);
    pub fn gtk_notebook_set_show_border(notebook: W, show: c_int);

    pub fn gtk_scrolled_window_new(hadjustment: W, vadjustment: W) -> W;
    pub fn gtk_scrolled_window_set_policy(window: W, horizontal: c_int, vertical: c_int);
    pub fn gtk_scrolled_window_set_min_content_height(window: W, height: c_int);

    pub fn gtk_text_view_new() -> W;
    pub fn gtk_text_view_get_buffer(view: W) -> W;
    pub fn gtk_text_view_set_editable(view: W, editable: c_int);
    pub fn gtk_text_view_set_cursor_visible(view: W, visible: c_int);
    pub fn gtk_text_view_set_monospace(view: W, monospace: c_int);
    pub fn gtk_text_view_set_wrap_mode(view: W, mode: c_int);
    pub fn gtk_text_view_set_left_margin(view: W, margin: c_int);
    pub fn gtk_text_view_set_right_margin(view: W, margin: c_int);
    pub fn gtk_text_view_set_top_margin(view: W, margin: c_int);
    pub fn gtk_text_view_set_bottom_margin(view: W, margin: c_int);
    pub fn gtk_text_view_scroll_to_mark(
        view: W,
        mark: W,
        within_margin: f64,
        use_align: c_int,
        xalign: f64,
        yalign: f64,
    );
    pub fn gtk_text_buffer_set_text(buffer: W, text: *const c_char, length: c_int);
    pub fn gtk_text_buffer_insert(buffer: W, iter: W, text: *const c_char, length: c_int);
    pub fn gtk_text_buffer_get_insert(buffer: W) -> W;
    pub fn gtk_text_buffer_get_end_iter(buffer: W, iter: W);
    pub fn gtk_text_buffer_create_mark(
        buffer: W,
        name: *const c_char,
        where_: W,
        left_gravity: c_int,
    ) -> W;
    pub fn gtk_text_buffer_get_mark(buffer: W, name: *const c_char) -> W;
    pub fn gtk_text_buffer_move_mark(buffer: W, mark: W, where_: W);

    pub fn gtk_css_provider_new() -> W;
    pub fn gtk_css_provider_load_from_data(
        provider: W,
        data: *const c_char,
        length: isize,
        error: *mut W,
    ) -> c_int;
    pub fn gtk_style_context_add_provider_for_screen(screen: W, provider: W, priority: c_uint);

    pub fn gtk_clipboard_get(selection: W) -> W;
    pub fn gtk_clipboard_set_text(clipboard: W, text: *const c_char, length: c_int);

    pub fn gtk_file_chooser_dialog_new(
        title: *const c_char,
        parent: W,
        action: c_int,
        first_button_text: *const c_char,
        ...
    ) -> W;
    pub fn gtk_file_chooser_get_filename(chooser: W) -> *mut c_char;
    pub fn gtk_file_chooser_set_current_folder(chooser: W, filename: *const c_char) -> c_int;

    pub fn gtk_dialog_new() -> W;
    pub fn gtk_dialog_run(dialog: W) -> c_int;
    pub fn gtk_dialog_add_button(dialog: W, text: *const c_char, response: c_int) -> W;
    pub fn gtk_dialog_get_content_area(dialog: W) -> W;
    pub fn gtk_dialog_set_default_response(dialog: W, response: c_int);
    pub fn gtk_message_dialog_new(
        parent: W,
        flags: c_int,
        message_type: c_int,
        buttons: c_int,
        message_format: *const c_char,
        ...
    ) -> W;
    pub fn gtk_widget_get_name(widget: W) -> *const c_char;
}

#[link(name = "gdk-3")]
unsafe extern "C" {
    pub fn gdk_screen_get_default() -> W;
    pub fn gdk_set_program_class(class: *const c_char);
    pub fn gdk_atom_intern_static_string(name: *const c_char) -> W;
    pub fn gdk_display_get_default() -> W;
    pub fn gdk_display_get_primary_monitor(display: W) -> W;
    pub fn gdk_display_get_monitor(display: W, index: c_int) -> W;
    pub fn gdk_monitor_get_workarea(monitor: W, rectangle: *mut Rectangle);
}

#[repr(C)]
#[derive(Default)]
pub struct Rectangle {
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
}

#[link(name = "gdk_pixbuf-2.0")]
unsafe extern "C" {
    pub fn gdk_pixbuf_loader_new_with_type(format: *const c_char, error: *mut W) -> W;
    pub fn gdk_pixbuf_loader_write(
        loader: W,
        bytes: *const u8,
        count: usize,
        error: *mut W,
    ) -> c_int;
    pub fn gdk_pixbuf_loader_close(loader: W, error: *mut W) -> c_int;
    pub fn gdk_pixbuf_loader_get_pixbuf(loader: W) -> W;
    pub fn gdk_pixbuf_scale_simple(
        pixbuf: W,
        width: c_int,
        height: c_int,
        interpolation: c_int,
    ) -> W;
}

#[link(name = "fontconfig")]
unsafe extern "C" {
    pub fn FcConfigAppFontAddFile(config: W, file: *const c_char) -> c_int;
}

#[link(name = "gobject-2.0")]
unsafe extern "C" {
    pub fn g_signal_connect_data(
        instance: W,
        signal: *const c_char,
        callback: *mut c_void,
        data: W,
        destroy: W,
        flags: c_uint,
    ) -> u64;
    pub fn g_object_unref(object: W);
    pub fn g_object_ref(object: W) -> W;
    pub fn g_object_set(object: W, first_property: *const c_char, ...);
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    pub fn g_timeout_add(
        interval: c_uint,
        callback: Option<unsafe extern "C" fn(W) -> c_int>,
        data: W,
    ) -> c_uint;
    pub fn g_idle_add(callback: Option<unsafe extern "C" fn(W) -> c_int>, data: W) -> c_uint;
    pub fn g_unix_signal_add(
        signum: c_int,
        handler: Option<unsafe extern "C" fn(W) -> c_int>,
        user_data: W,
    ) -> c_uint;
    pub fn g_source_remove(tag: c_uint) -> c_int;
    pub fn g_free(pointer: W);
    pub fn g_set_prgname(name: *const c_char);
    pub fn g_set_application_name(name: *const c_char);
}

#[link(name = "cairo")]
unsafe extern "C" {
    pub fn cairo_image_surface_create(format: c_int, width: c_int, height: c_int) -> W;
    pub fn cairo_create(surface: W) -> W;
    pub fn cairo_destroy(cairo: W);
    pub fn cairo_status(cairo: W) -> c_int;
    pub fn cairo_surface_destroy(surface: W);
    pub fn cairo_surface_write_to_png(surface: W, path: *const c_char) -> c_int;
    pub fn cairo_new_path(cairo: W);
    pub fn cairo_close_path(cairo: W);
    pub fn cairo_arc(cairo: W, x: f64, y: f64, radius: f64, angle1: f64, angle2: f64);
    pub fn cairo_set_source_rgba(cairo: W, red: f64, green: f64, blue: f64, alpha: f64);
    pub fn cairo_fill(cairo: W);
    pub fn cairo_fill_preserve(cairo: W);
    pub fn cairo_stroke(cairo: W);
    pub fn cairo_set_line_width(cairo: W, width: f64);
}

unsafe extern "C" {
    pub fn kill(pid: c_int, signal: c_int) -> c_int;
    pub fn killpg(pgid: c_int, signal: c_int) -> c_int;
    pub fn getpgid(pid: c_int) -> c_int;
    pub fn setsid() -> c_int;
    pub fn dup2(oldfd: c_int, newfd: c_int) -> c_int;
    pub fn geteuid() -> c_uint;
    pub fn flock(fd: c_int, operation: c_int) -> c_int;
    pub fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Tm {
    pub tm_sec: c_int,
    pub tm_min: c_int,
    pub tm_hour: c_int,
    pub tm_mday: c_int,
    pub tm_mon: c_int,
    pub tm_year: c_int,
    pub tm_wday: c_int,
    pub tm_yday: c_int,
    pub tm_isdst: c_int,
    pub tm_gmtoff: c_long,
    pub tm_zone: *const c_char,
}
