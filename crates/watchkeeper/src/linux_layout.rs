//! Window layout on Linux.
//!
//! On Linux, Tauri 2.12 stacks a window's child webviews in a vertical
//! GTK box and ignores the positions and sizes asked for, so the page,
//! toolbar and panel come out as three equal rows. Here the GTK widgets are
//! rearranged directly: the toolbar keeps a fixed height at the top, and a
//! horizontal box below it holds the page (which takes the spare room) and
//! the panel (fixed width). GTK then handles resizing by itself.

use gtk::prelude::*;
use std::sync::mpsc;

use crate::{Measured, PAGE, PANEL, PANEL_WIDTH, TOOLBAR, TOOLBAR_HEIGHT};

fn on_main<T: Send + 'static>(
    window: &tauri::Window,
    f: impl FnOnce(tauri::Window) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    let w = window.clone();
    window
        .run_on_main_thread(move || {
            let _ = tx.send(f(w));
        })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

/// Moves the three webviews into place. Call once, after all three have
/// been added in the order toolbar, page, panel.
pub fn arrange(window: &tauri::Window) -> Result<(), String> {
    on_main(window, |w| {
        let vbox = w.default_vbox().map_err(|e| e.to_string())?;
        let kids = vbox.children();
        if kids.len() != 3 {
            return Err(format!(
                "expected 3 webviews in the window, found {}",
                kids.len()
            ));
        }
        let (bar, page, panel) = (kids[0].clone(), kids[1].clone(), kids[2].clone());
        vbox.remove(&page);
        vbox.remove(&panel);

        bar.set_size_request(-1, TOOLBAR_HEIGHT as i32);
        vbox.set_child_packing(&bar, false, false, 0, gtk::PackType::Start);

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.pack_start(&page, true, true, 0);
        panel.set_size_request(PANEL_WIDTH as i32, -1);
        body.pack_start(&panel, false, false, 0);
        vbox.pack_start(&body, true, true, 0);
        vbox.show_all();
        Ok(())
    })
}

/// Where each webview actually is, in logical pixels relative to the
/// window's content area, plus that area's width and height.
pub fn measure(window: &tauri::Window) -> Result<Measured, String> {
    on_main(window, |w| {
        let vbox = w.default_vbox().map_err(|e| e.to_string())?;
        let kids = vbox.children();
        let body = kids
            .get(1)
            .and_then(|b| b.clone().downcast::<gtk::Box>().ok())
            .ok_or("layout was not arranged")?;
        let inner = body.children();
        let widgets = [
            (TOOLBAR, kids[0].clone()),
            (PAGE, inner.first().cloned().ok_or("no page widget")?),
            (PANEL, inner.get(1).cloned().ok_or("no panel widget")?),
        ];
        let mut rects = Vec::new();
        for (label, widget) in widgets {
            let (x, y) = widget
                .translate_coordinates(&vbox, 0, 0)
                .ok_or(format!("{label} is not inside the window"))?;
            rects.push((
                label,
                (
                    x as f64,
                    y as f64,
                    widget.allocated_width() as f64,
                    widget.allocated_height() as f64,
                ),
            ));
        }
        Ok((
            vbox.allocated_width() as f64,
            vbox.allocated_height() as f64,
            rects,
        ))
    })
}
