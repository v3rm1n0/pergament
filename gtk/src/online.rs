//! "Search online" page: browse the public catalog and download publications.
//! Network and catalog work runs on worker threads.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};
use jwlinux::Library;
use jwlinux::catalog::{Catalog, CatalogItem};
use jwlinux::net::{Client, HttpConfig};
use jwlinux::remote::{self, Request};

use crate::window::{Ui, refresh_library, scrolled, toast};

thread_local! {
    static CATALOG: std::cell::RefCell<Option<Arc<Mutex<Catalog>>>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone)]
struct Page {
    lang: gtk::Entry,
    query: gtk::SearchEntry,
    progress: gtk::ProgressBar,
    results: gtk::ListBox,
    status: adw::StatusPage,
    stack: gtk::Stack,
}

fn default_lang() -> &'static str {
    let lang = std::env::var("LANG").unwrap_or_default();
    if lang.starts_with("de") { "X" } else { "E" }
}

pub fn show(ui: &Ui) {
    if ui
        .sidebar_nav
        .visible_page()
        .is_some_and(|p| p.tag().as_deref() == Some("online"))
    {
        return;
    }
    let lang = gtk::Entry::builder()
        .text(default_lang())
        .max_width_chars(5)
        .width_chars(5)
        .tooltip_text("Language code, e.g. X (German), E (English)")
        .build();
    let query = gtk::SearchEntry::builder()
        .placeholder_text("Title or symbol")
        .hexpand(true)
        .build();
    let bar = gtk::Box::builder()
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    bar.append(&lang);
    bar.append(&query);
    let progress = gtk::ProgressBar::builder()
        .show_text(true)
        .visible(false)
        .margin_start(12)
        .margin_end(12)
        .build();
    let results = gtk::ListBox::new();
    results.add_css_class("navigation-sidebar");
    let status = adw::StatusPage::builder()
        .icon_name("system-search-symbolic")
        .title("Search online")
        .description(
            "Searches the public jw.org catalog. The first search downloads it (about 58 MB).",
        )
        .build();
    let stack = gtk::Stack::new();
    stack.add_named(&status, Some("status"));
    stack.add_named(&scrolled(&results), Some("results"));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.append(&bar);
    content.append(&progress);
    content.append(&stack);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&content));
    let nav_page = adw::NavigationPage::builder()
        .title("Search online")
        .tag("online")
        .child(&view)
        .build();

    let page = Page {
        lang,
        query,
        progress,
        results,
        status,
        stack,
    };
    let run = glib::clone!(
        #[strong]
        ui,
        #[strong]
        page,
        move || search(&ui, &page)
    );
    page.query.connect_activate({
        let run = run.clone();
        move |_| run()
    });
    page.lang.connect_activate(move |_| run());
    ui.sidebar_nav.push(&nav_page);
    page.query.grab_focus();
}

/// Progress channel from a worker thread to a progress bar.
fn progress_channel(
    bar: &gtk::ProgressBar,
    label: &str,
) -> async_channel::Sender<(u64, Option<u64>)> {
    let (tx, rx) = async_channel::unbounded::<(u64, Option<u64>)>();
    bar.set_visible(true);
    bar.set_fraction(0.0);
    bar.set_text(Some(label));
    let bar = bar.clone();
    let label = label.to_owned();
    glib::spawn_future_local(async move {
        while let Ok((done, total)) = rx.recv().await {
            match total {
                Some(t) if t > 0 => {
                    bar.set_fraction(done as f64 / t as f64);
                    bar.set_text(Some(&format!(
                        "{label} {:.1}/{:.1} MB",
                        done as f64 / 1e6,
                        t as f64 / 1e6
                    )));
                }
                _ => {
                    bar.pulse();
                    bar.set_text(Some(&format!("{label} {:.1} MB", done as f64 / 1e6)));
                }
            }
        }
        bar.set_visible(false);
    });
    tx
}

fn search(ui: &Ui, page: &Page) {
    let code = page.lang.text().trim().to_owned();
    let query = page.query.text().trim().to_owned();
    if code.is_empty() {
        return;
    }
    let cache = ui.state.borrow().cache.clone();
    let loaded = CATALOG.with(|c| c.borrow().clone());
    let ui = ui.clone();
    let page = page.clone();
    glib::spawn_future_local(async move {
        let catalog = match loaded {
            Some(c) => c,
            None => {
                let tx = progress_channel(&page.progress, "Catalog");
                let res = gio::spawn_blocking(move || {
                    let client = Client::new(HttpConfig::default());
                    Catalog::load(
                        &client,
                        &cache,
                        false,
                        Duration::from_secs(24 * 3600),
                        &mut |d, t| {
                            let _ = tx.send_blocking((d, t));
                        },
                    )
                    .map(|c| Arc::new(Mutex::new(c)))
                    .map_err(|e| e.to_string())
                })
                .await;
                match res {
                    Ok(Ok(c)) => {
                        CATALOG.with(|slot| *slot.borrow_mut() = Some(c.clone()));
                        c
                    }
                    Ok(Err(e)) => return toast(&ui, &format!("Catalog: {e}")),
                    Err(_) => return toast(&ui, "Catalog download failed"),
                }
            }
        };
        let res = gio::spawn_blocking(move || {
            let cat = catalog
                .lock()
                .map_err(|_| "catalog lock poisoned".to_owned())?;
            let meps = cat
                .meps_language(&code)
                .ok_or_else(|| format!("Unknown language code {code:?}"))?;
            cat.search(meps, &query, 200)
                .map(|items| (code, items))
                .map_err(|e| e.to_string())
        })
        .await;
        match res {
            Ok(Ok((code, items))) => show_results(&ui, &page, &code, items),
            Ok(Err(e)) => toast(&ui, &e),
            Err(_) => toast(&ui, "Search failed"),
        }
    });
}

fn show_results(ui: &Ui, page: &Page, code: &str, items: Vec<CatalogItem>) {
    while let Some(row) = page.results.first_child() {
        page.results.remove(&row);
    }
    if items.is_empty() {
        page.status.set_title("Nothing found");
        page.status.set_description(None);
        page.stack.set_visible_child_name("status");
        return;
    }
    for item in items {
        let mut subtitle = format!("{} · {:.1} MB", item.symbol, item.size as f64 / 1e6);
        if let Some(t) = item.issue_title.as_deref().filter(|t| !t.is_empty()) {
            subtitle = format!("{t} · {subtitle}");
        }
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&item.title))
            .subtitle(glib::markup_escape_text(&subtitle))
            .build();
        let button = gtk::Button::builder()
            .icon_name("folder-download-symbolic")
            .tooltip_text("Download and import")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        button.connect_clicked(glib::clone!(
            #[strong]
            ui,
            #[strong]
            page,
            #[strong]
            item,
            #[to_owned]
            code,
            move |b| download(&ui, &page, b, &code, &item)
        ));
        row.add_suffix(&button);
        page.results.append(&row);
    }
    page.stack.set_visible_child_name("results");
}

fn download(ui: &Ui, page: &Page, button: &gtk::Button, code: &str, item: &CatalogItem) {
    button.set_sensitive(false);
    let (root, cache) = {
        let s = ui.state.borrow();
        (s.library_root.clone(), s.cache.clone())
    };
    let tx = progress_channel(&page.progress, &item.symbol);
    let (code, item) = (code.to_owned(), item.clone());
    let ui = ui.clone();
    let button = button.clone();
    glib::spawn_future_local(async move {
        let res = gio::spawn_blocking(move || {
            let client = Client::new(HttpConfig::default());
            let mut lib = Library::open(&root).map_err(|e| e.to_string())?;
            let req = Request {
                key_symbol: &item.key_symbol,
                lang_code: &code,
                issue_tag: Some(item.issue_tag),
            };
            remote::download(
                &client,
                &mut lib,
                &cache.join("downloads"),
                &req,
                Some(&item),
                &mut |d, t| {
                    let _ = tx.send_blocking((d, t));
                },
            )
            .map(|e| e.title)
            .map_err(|e| e.to_string())
        })
        .await;
        button.set_sensitive(true);
        match res {
            Ok(Ok(title)) => {
                toast(&ui, &format!("Imported {title}"));
                button.set_icon_name("object-select-symbolic");
                refresh_library(&ui);
            }
            Ok(Err(e)) => toast(&ui, &format!("Download failed: {e}")),
            Err(_) => toast(&ui, "Download failed"),
        }
    });
}
