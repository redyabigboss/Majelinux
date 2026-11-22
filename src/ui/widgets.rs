use std::ffi::{CStr, CString, c_int, c_void};
use std::ptr;

use crate::ffi::*;

pub fn c(text: &str) -> CString {
    match CString::new(text) {
        Ok(value) => value,
        Err(_) => {
            let bytes: Vec<u8> = text.bytes().filter(|byte| *byte != 0).collect();
            CString::new(bytes).unwrap_or_default()
        }
    }
}

pub unsafe fn style(widget: W, class: &str) {
    unsafe {
        let context = gtk_widget_get_style_context(widget);
        gtk_style_context_add_class(context, c(class).as_ptr());
    }
}

pub unsafe fn add_class(widget: W, class: &str) {
    unsafe {
        let context = gtk_widget_get_style_context(widget);
        gtk_style_context_add_class(context, c(class).as_ptr());
    }
}

pub unsafe fn remove_class(widget: W, class: &str) {
    unsafe {
        let context = gtk_widget_get_style_context(widget);
        gtk_style_context_remove_class(context, c(class).as_ptr());
    }
}

pub unsafe fn button_new() -> W {
    unsafe {
        let button = gtk_button_new();
        gtk_widget_set_focus_on_click(button, 0);
        button
    }
}

pub unsafe fn button_label(text: &str) -> W {
    unsafe {
        let button = gtk_button_new_with_label(c(text).as_ptr());
        gtk_widget_set_focus_on_click(button, 0);
        button
    }
}

pub unsafe fn label(text: &str, class: &str) -> W {
    unsafe {
        let widget = gtk_label_new(c(text).as_ptr());
        gtk_label_set_xalign(widget, 0.0);
        gtk_label_set_line_wrap(widget, 1);
        if !class.is_empty() {
            style(widget, class);
        }
        widget
    }
}

pub unsafe fn centered_label(text: &str, class: &str) -> W {
    unsafe {
        let widget = label(text, class);
        gtk_label_set_xalign(widget, 0.5);
        widget
    }
}

pub unsafe fn set_text(widget: W, text: &str) {
    unsafe { gtk_label_set_text(widget, c(text).as_ptr()) };
}

pub unsafe fn set_markup(widget: W, text: &str) {
    unsafe { gtk_label_set_markup(widget, c(text).as_ptr()) };
}

pub unsafe fn pack(container: W, child: W, expand: bool) {
    unsafe { gtk_box_pack_start(container, child, expand as c_int, 1, 0) };
}

pub unsafe fn box_new(vertical: bool, spacing: c_int) -> W {
    unsafe { gtk_box_new(vertical as c_int, spacing) }
}

pub unsafe fn connect(widget: W, signal: &str, callback: *const c_void, data: W) {
    unsafe {
        g_signal_connect_data(
            widget,
            c(signal).as_ptr(),
            callback as *mut c_void,
            data,
            ptr::null_mut(),
            0,
        );
    }
}

fn icon_path(name: &str) -> &'static str {
    match name {
        "play" => "M8 5 19 12 8 19Z",
        "download" => "M12 3v12m-5-5 5 5 5-5M4 20h16",
        "tune" => "M4 7h5m5 0h6M4 17h10m5 0h1M9 4v6m5 4v6",
        "plus" => "M12 5v14M5 12h14",
        "info" => "M12 11v6M12 7.5v.5",
        "open" => "M14 4h6v6M20 4l-9 9M10 4H4v16h16v-6",
        "stop" => "M7 7h10v10H7Z",
        "copy" => "M9 9h11v11H9ZM5 15H4V4h11v1",
        "check" => "M5 12.5l4.3 4.3L19 7",
        "dot" => "M12 5a7 7 0 1 0 0 14 7 7 0 1 0 0-14Z",
        "cross" => "M6.5 6.5l11 11M17.5 6.5l-11 11",
        "warn" => "M12 7v6M12 17v.5",
        _ => "M5 12h14M12 5l7 7-7 7",
    }
}

pub unsafe fn symbol_pixbuf(name: &str, color: &str, size: c_int) -> W {
    let path = icon_path(name);
    let width = if matches!(name, "check" | "cross" | "dot" | "warn") {
        2.7
    } else {
        1.8
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><path d="{path}" fill="none" stroke="{color}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
    );
    unsafe {
        let loader = gdk_pixbuf_loader_new_with_type(c("svg").as_ptr(), ptr::null_mut());
        if loader.is_null() {
            return ptr::null_mut();
        }
        let mut result = ptr::null_mut();
        if gdk_pixbuf_loader_write(loader, svg.as_ptr(), svg.len(), ptr::null_mut()) != 0
            && gdk_pixbuf_loader_close(loader, ptr::null_mut()) != 0
        {
            let pixbuf = gdk_pixbuf_loader_get_pixbuf(loader);
            if !pixbuf.is_null() {
                result = gdk_pixbuf_scale_simple(pixbuf, size, size, 2);
            }
        }
        g_object_unref(loader);
        result
    }
}

pub unsafe fn symbol(name: &str, color: &str, size: c_int) -> W {
    unsafe {
        let pixbuf = symbol_pixbuf(name, color, size);
        if pixbuf.is_null() {
            return label("", "caption");
        }
        let image = gtk_image_new_from_pixbuf(pixbuf);
        g_object_unref(pixbuf);
        image
    }
}

pub unsafe fn png_image(bytes: &[u8], size: c_int) -> W {
    unsafe {
        let loader = gdk_pixbuf_loader_new_with_type(c("png").as_ptr(), ptr::null_mut());
        if loader.is_null() {
            return label("", "caption");
        }
        let mut result = ptr::null_mut();
        if gdk_pixbuf_loader_write(loader, bytes.as_ptr(), bytes.len(), ptr::null_mut()) != 0
            && gdk_pixbuf_loader_close(loader, ptr::null_mut()) != 0
        {
            let pixbuf = gdk_pixbuf_loader_get_pixbuf(loader);
            if !pixbuf.is_null() {
                let scaled = gdk_pixbuf_scale_simple(pixbuf, size, size, 2);
                if !scaled.is_null() {
                    result = gtk_image_new_from_pixbuf(scaled);
                    g_object_unref(scaled);
                }
            }
        }
        g_object_unref(loader);
        if result.is_null() {
            label("", "caption")
        } else {
            result
        }
    }
}

pub unsafe fn append_log(view: W, buffer: W, text: &str) {
    if text.is_empty() {
        return;
    }
    unsafe {
        let mut iter = TextIter::new();
        gtk_text_buffer_get_end_iter(buffer, iter.as_mut_ptr());
        gtk_text_buffer_insert(buffer, iter.as_mut_ptr(), c(text).as_ptr(), -1);
        scroll_log(view, buffer);
    }
}

pub unsafe fn replace_log(view: W, buffer: W, text: &str) {
    unsafe {
        gtk_text_buffer_set_text(buffer, c(text).as_ptr(), -1);
        scroll_log(view, buffer);
    }
}

unsafe fn scroll_log(view: W, buffer: W) {
    unsafe {
        let mut iter = TextIter::new();
        gtk_text_buffer_get_end_iter(buffer, iter.as_mut_ptr());
        let mark = gtk_text_buffer_create_mark(buffer, c("tail").as_ptr(), iter.as_mut_ptr(), 0);
        gtk_text_view_scroll_to_mark(view, mark, 0.0, 0, 0.0, 1.0);
    }
}

#[repr(C, align(8))]
pub struct TextIter([u8; 128]);

impl TextIter {
    pub fn new() -> Self {
        Self([0u8; 128])
    }
    pub fn as_mut_ptr(&mut self) -> W {
        self.0.as_mut_ptr().cast()
    }
}

pub unsafe fn message(parent: W, kind: c_int, text: &str) {
    unsafe {
        let dialog =
            gtk_message_dialog_new(parent, 1 | 2, kind, 1, c("%s").as_ptr(), c(text).as_ptr());
        gtk_dialog_run(dialog);
        gtk_widget_destroy(dialog);
    }
}

pub unsafe fn confirm(parent: W, text: &str) -> bool {
    unsafe {
        let dialog =
            gtk_message_dialog_new(parent, 1 | 2, 2, 4, c("%s").as_ptr(), c(text).as_ptr());
        let response = gtk_dialog_run(dialog);
        gtk_widget_destroy(dialog);
        response == -8
    }
}

pub unsafe fn clipboard(text: &str) {
    unsafe {
        let atom = gdk_atom_intern_static_string(c("CLIPBOARD").as_ptr());
        let board = gtk_clipboard_get(atom);
        gtk_clipboard_set_text(board, c(text).as_ptr(), -1);
    }
}

pub unsafe fn entry_text(entry: W) -> String {
    unsafe {
        let raw = gtk_entry_get_text(entry);
        if raw.is_null() {
            String::new()
        } else {
            CStr::from_ptr(raw).to_string_lossy().into_owned()
        }
    }
}

pub unsafe fn button_icon(button: W, icon: W) {
    unsafe {
        gtk_button_set_image(button, icon);
        gtk_button_set_always_show_image(button, 1);
        gtk_button_set_image_position(button, 2);
    }
}

pub unsafe fn log_margins(view: W) {
    unsafe {
        gtk_text_view_set_left_margin(view, 14);
        gtk_text_view_set_right_margin(view, 14);
        gtk_text_view_set_top_margin(view, 12);
        gtk_text_view_set_bottom_margin(view, 12);
    }
}

pub unsafe fn choose_folder(
    parent: W,
    title: &str,
    accept: &str,
    cancel: &str,
    start: &str,
) -> Option<String> {
    unsafe {
        let dialog = gtk_file_chooser_dialog_new(
            c(title).as_ptr(),
            parent,
            2,
            c(cancel).as_ptr(),
            -6 as c_int,
            c(accept).as_ptr(),
            -3 as c_int,
            ptr::null::<std::ffi::c_char>(),
        );
        if dialog.is_null() {
            return None;
        }
        gtk_file_chooser_set_current_folder(dialog, c(start).as_ptr());
        let response = gtk_dialog_run(dialog);
        let mut result = None;
        if response == -3 {
            let raw = gtk_file_chooser_get_filename(dialog);
            if !raw.is_null() {
                result = Some(CStr::from_ptr(raw).to_string_lossy().into_owned());
                g_free(raw.cast());
            }
        }
        gtk_widget_destroy(dialog);
        result
    }
}
