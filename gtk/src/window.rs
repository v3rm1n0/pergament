//! Main window: library and table of contents in the sidebar, reader on the
//! right.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use jwlinux::links::Link;
use jwlinux::navigate::{self, Target, TargetKind};
use jwlinux::{Entry, Library, Publication};
use webkit6::prelude::*;

use crate::online;

pub struct State {
    pub library: Library,
    pub library_root: PathBuf,
    pub cache: PathBuf,
    /// Publication of the page currently shown.
    pub current: Option<Entry>,
}

#[derive(Clone)]
pub struct Ui {
    pub window: adw::ApplicationWindow,
    pub state: Rc<RefCell<State>>,
    pub sidebar_nav: adw::NavigationView,
    pub split: adw::NavigationSplitView,
    pub library_list: gtk::ListBox,
    pub web: webkit6::WebView,
    pub toasts: adw::ToastOverlay,
    pub content_page: adw::NavigationPage,
}

pub fn build(
    app: &adw::Application,
    initial: Option<(String, Option<String>)>,
) -> Result<adw::ApplicationWindow, String> {
    let library_root = std::env::var_os("JWL_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("jwlinux")))
        .ok_or("no data directory")?;
    let cache = std::env::var_os("JWL_CACHE")
        .map(PathBuf::from)
        .or_else(|| dirs::cache_dir().map(|d| d.join("jwlinux")))
        .ok_or("no cache directory")?;
    let library = Library::open(&library_root).map_err(|e| e.to_string())?;

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("jwlinux")
        .default_width(1200)
        .default_height(820)
        .build();

    // Reader
    let web = webkit6::WebView::new();
    if let Some(settings) = webkit6::prelude::WebViewExt::settings(&web) {
        // Content is sanitized and needs no scripts.
        settings.set_enable_javascript(false);
        settings.set_javascript_can_open_windows_automatically(false);
        settings.set_enable_developer_extras(false);
        settings.set_allow_file_access_from_file_urls(true);
        settings.set_allow_universal_access_from_file_urls(false);
    }
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_action_name(Some("win.back"));
    back.set_tooltip_text(Some("Back"));
    let forward = gtk::Button::from_icon_name("go-next-symbolic");
    forward.set_action_name(Some("win.forward"));
    forward.set_tooltip_text(Some("Forward"));
    let content_header = adw::HeaderBar::new();
    content_header.pack_start(&back);
    content_header.pack_start(&forward);
    let content_view = adw::ToolbarView::new();
    content_view.add_top_bar(&content_header);
    let placeholder = adw::StatusPage::builder()
        .icon_name("accessories-dictionary-symbolic")
        .title("jwlinux")
        .description("Choose a publication on the left, import a .jwpub file (Ctrl+O) or search online (Ctrl+F).")
        .build();
    let stack = gtk::Stack::new();
    stack.add_named(&placeholder, Some("empty"));
    stack.add_named(&web, Some("web"));
    content_view.set_content(Some(&stack));
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&content_view));
    let content_page = adw::NavigationPage::builder()
        .title("Reader")
        .child(&toasts)
        .build();

    // Sidebar
    let sidebar_nav = adw::NavigationView::new();
    let library_list = gtk::ListBox::new();
    library_list.add_css_class("navigation-sidebar");
    sidebar_nav.add(&library_page(&library_list));
    let sidebar_page = adw::NavigationPage::builder()
        .title("Library")
        .child(&sidebar_nav)
        .build();

    let split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .min_sidebar_width(300.0)
        .max_sidebar_width(380.0)
        .build();
    window.set_content(Some(&split));

    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        700.0,
        adw::LengthUnit::Sp,
    ));
    breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(breakpoint);

    let ui = Ui {
        window: window.clone(),
        state: Rc::new(RefCell::new(State {
            library,
            library_root,
            cache,
            current: None,
        })),
        sidebar_nav,
        split,
        library_list,
        web: web.clone(),
        toasts,
        content_page,
    };

    // Show the web view once something is loaded; keep the title in sync.
    web.connect_load_changed(glib::clone!(
        #[weak]
        stack,
        move |_, _| stack.set_visible_child_name("web")
    ));
    web.connect_title_notify(glib::clone!(
        #[strong]
        ui,
        move |w| {
            if let Some(t) = w.title() {
                ui.content_page.set_title(&t);
            }
        }
    ));
    web.connect_decide_policy(glib::clone!(
        #[strong]
        ui,
        move |_, decision, kind| decide_policy(&ui, decision, kind)
    ));

    install_actions(&ui, &back, &forward);
    refresh_library(&ui);
    if let Some((publication, doc)) = initial {
        open_initial(&ui, &publication, doc.as_deref());
    }
    Ok(window)
}

fn library_page(list: &gtk::ListBox) -> adw::NavigationPage {
    let header = adw::HeaderBar::new();
    let import = gtk::Button::from_icon_name("document-open-symbolic");
    import.set_action_name(Some("win.import"));
    import.set_tooltip_text(Some("Import .jwpub (Ctrl+O)"));
    let search = gtk::Button::from_icon_name("system-search-symbolic");
    search.set_action_name(Some("win.search-online"));
    search.set_tooltip_text(Some("Search and download (Ctrl+F)"));
    let menu = gio::Menu::new();
    let theme = gio::Menu::new();
    theme.append(Some("Follow system"), Some("app.theme::system"));
    theme.append(Some("Light"), Some("app.theme::light"));
    theme.append(Some("Dark"), Some("app.theme::dark"));
    menu.append_section(Some("Style"), &theme);
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text("Menu")
        .build();
    header.pack_start(&import);
    header.pack_end(&menu_button);
    header.pack_end(&search);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&scrolled(list)));
    adw::NavigationPage::builder()
        .title("Library")
        .tag("library")
        .child(&view)
        .build()
}

pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(child)
        .build()
}

fn install_actions(ui: &Ui, back: &gtk::Button, forward: &gtk::Button) {
    let win = &ui.window;
    let action = gio::SimpleAction::new("back", None);
    action.connect_activate(glib::clone!(
        #[weak(rename_to = web)]
        ui.web,
        move |_, _| web.go_back()
    ));
    win.add_action(&action);
    let action = gio::SimpleAction::new("forward", None);
    action.connect_activate(glib::clone!(
        #[weak(rename_to = web)]
        ui.web,
        move |_, _| web.go_forward()
    ));
    win.add_action(&action);
    let update = glib::clone!(
        #[weak]
        back,
        #[weak]
        forward,
        move |w: &webkit6::WebView| {
            back.set_sensitive(w.can_go_back());
            forward.set_sensitive(w.can_go_forward());
        }
    );
    update(&ui.web);
    ui.web.connect_load_changed(move |w, _| update(w));

    let action = gio::SimpleAction::new("import", None);
    action.connect_activate(glib::clone!(
        #[strong]
        ui,
        move |_, _| import_dialog(&ui)
    ));
    win.add_action(&action);

    let action = gio::SimpleAction::new("search-online", None);
    action.connect_activate(glib::clone!(
        #[strong]
        ui,
        move |_, _| online::show(&ui)
    ));
    win.add_action(&action);
}

pub fn toast(ui: &Ui, text: &str) {
    ui.toasts.add_toast(adw::Toast::new(text));
}

/// Rebuild the publication list from the library index.
pub fn refresh_library(ui: &Ui) {
    let list = &ui.library_list;
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }
    let entries = match ui.state.borrow().library.list() {
        Ok(e) => e,
        Err(e) => {
            toast(ui, &format!("Cannot read library: {e}"));
            return;
        }
    };
    if entries.is_empty() {
        let row = adw::ActionRow::builder()
            .title("No publications yet")
            .subtitle("Import a .jwpub file or search online")
            .activatable(false)
            .build();
        list.append(&row);
        return;
    }
    for entry in entries {
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&entry.title))
            .subtitle(format!("{} · {}", entry.symbol, entry.year))
            .activatable(true)
            .build();
        row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        let remove = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Remove from library")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        remove.connect_clicked(glib::clone!(
            #[strong]
            ui,
            #[strong]
            entry,
            move |_| confirm_remove(&ui, &entry)
        ));
        row.add_prefix(&remove);
        row.connect_activated(glib::clone!(
            #[strong]
            ui,
            #[strong]
            entry,
            move |_| show_publication(&ui, &entry)
        ));
        list.append(&row);
    }
}

fn confirm_remove(ui: &Ui, entry: &Entry) {
    let dialog = adw::AlertDialog::new(
        Some("Remove publication?"),
        Some(&format!(
            "“{}” will be deleted from the library.",
            entry.title
        )),
    );
    dialog.add_responses(&[("cancel", "Cancel"), ("remove", "Remove")]);
    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.connect_response(
        None,
        glib::clone!(
            #[strong]
            ui,
            #[strong]
            entry,
            move |_, response| {
                if response != "remove" {
                    return;
                }
                let res = ui.state.borrow_mut().library.remove(&entry);
                match res {
                    Ok(_) => toast(&ui, &format!("Removed {}", entry.title)),
                    Err(e) => toast(&ui, &format!("Remove failed: {e}")),
                }
                refresh_library(&ui);
            }
        ),
    );
    dialog.present(Some(&ui.window));
}

fn import_dialog(ui: &Ui) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("JW publications (*.jwpub)"));
    filter.add_pattern("*.jwpub");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder()
        .title("Import publications")
        .filters(&filters)
        .default_filter(&filter)
        .build();
    dialog.open_multiple(
        Some(&ui.window),
        gio::Cancellable::NONE,
        glib::clone!(
            #[strong]
            ui,
            move |res| {
                let Ok(files) = res else { return };
                let paths: Vec<PathBuf> = (0..files.n_items())
                    .filter_map(|i| files.item(i)?.downcast::<gio::File>().ok()?.path())
                    .collect();
                import_files(&ui, paths);
            }
        ),
    );
}

/// Import in a background thread with its own library handle.
pub fn import_files(ui: &Ui, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    toast(ui, &format!("Importing {} file(s)…", paths.len()));
    let root = ui.state.borrow().library_root.clone();
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || {
            let mut lib = Library::open(&root).map_err(|e| e.to_string())?;
            let mut done = Vec::new();
            for p in paths {
                match lib.import(&p) {
                    Ok(e) => done.push(Ok(e.title)),
                    Err(e) => done.push(Err(format!("{}: {e}", p.display()))),
                }
            }
            Ok::<_, String>(done)
        })
        .await;
        match result {
            Ok(Ok(results)) => {
                for r in results {
                    match r {
                        Ok(title) => toast(&ui, &format!("Imported {title}")),
                        Err(e) => toast(&ui, &format!("Import failed: {e}")),
                    }
                }
            }
            Ok(Err(e)) => toast(&ui, &format!("Import failed: {e}")),
            Err(_) => toast(&ui, "Import failed"),
        }
        refresh_library(&ui);
    });
}

/// Push a table of contents for a publication onto the sidebar.
pub fn show_publication(ui: &Ui, entry: &Entry) {
    let state = ui.state.borrow();
    let publication = match Publication::open(&state.library, entry) {
        Ok(p) => p,
        Err(e) => {
            drop(state);
            toast(ui, &format!("Cannot open {}: {e}", entry.title));
            return;
        }
    };
    let page_box = adw::PreferencesPage::new();

    if publication.is_bible() {
        let group = adw::PreferencesGroup::builder().title("Books").build();
        for book in publication.bible_books().unwrap_or_default() {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&book.title))
                .subtitle(format!("{} chapters", book.chapters))
                .activatable(true)
                .build();
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            row.connect_activated(glib::clone!(
                #[strong]
                ui,
                #[strong]
                entry,
                #[strong]
                book,
                move |_| show_chapters(&ui, &entry, &book)
            ));
            group.add(&row);
        }
        page_box.add(&group);
    }

    let group = adw::PreferencesGroup::builder().title("Documents").build();
    for doc in publication
        .documents()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d.has_content)
    {
        let title = if doc.title.is_empty() {
            format!("Document {}", doc.id)
        } else {
            doc.title.clone()
        };
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&title))
            .activatable(true)
            .build();
        row.connect_activated(glib::clone!(
            #[strong]
            ui,
            #[strong]
            entry,
            move |_| open(
                &ui,
                &Target {
                    publication: entry.dir_name.clone(),
                    kind: TargetKind::Document(doc.id),
                }
            )
        ));
        group.add(&row);
    }
    page_box.add(&group);
    drop(state);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&page_box));
    let page = adw::NavigationPage::builder()
        .title(entry.short_title.as_deref().unwrap_or(&entry.title))
        .child(&view)
        .build();
    ui.sidebar_nav.push(&page);
}

fn show_chapters(ui: &Ui, entry: &Entry, book: &jwlinux::reader::BibleBook) {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(8)
        .min_children_per_line(4)
        .row_spacing(6)
        .column_spacing(6)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .homogeneous(true)
        .valign(gtk::Align::Start)
        .build();
    for ch in 1..=book.chapters {
        let button = gtk::Button::with_label(&ch.to_string());
        button.connect_clicked(glib::clone!(
            #[strong]
            ui,
            #[strong]
            entry,
            #[strong(rename_to = number)]
            book.number,
            move |_| open(
                &ui,
                &Target {
                    publication: entry.dir_name.clone(),
                    kind: TargetKind::Chapter {
                        book: number,
                        chapter: ch,
                        verse: 1,
                    },
                }
            )
        ));
        flow.append(&button);
    }
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&scrolled(&flow)));
    let page = adw::NavigationPage::builder()
        .title(&book.title)
        .child(&view)
        .build();
    ui.sidebar_nav.push(&page);
}

/// Render a target into the view cache and load it.
pub fn open(ui: &Ui, target: &Target) {
    let (page, entry, view_dir) = {
        let state = ui.state.borrow();
        let page = navigate::page(&state.library, target);
        let entry = state.library.get_by_dir(&target.publication).ok().flatten();
        (page, entry, state.cache.join("view"))
    };
    let page = match page {
        Ok(p) => p,
        Err(e) => {
            toast(ui, &format!("Cannot show page: {e}"));
            return;
        }
    };
    let path = view_dir.join(format!("{}.html", page.name));
    if let Err(e) =
        std::fs::create_dir_all(&view_dir).and_then(|_| std::fs::write(&path, &page.html))
    {
        toast(ui, &format!("Cannot write page: {e}"));
        return;
    }
    ui.state.borrow_mut().current = entry;
    let mut uri = gio::File::for_path(&path).uri().to_string();
    if let Some(f) = page.fragment {
        uri.push('#');
        uri.push_str(&f);
    }
    ui.web.load_uri(&uri);
    ui.split.set_show_content(true);
}

/// Route link clicks: our pages and in-page anchors load normally,
/// `jwlinux:` links are resolved in the library, web links go to the browser.
fn decide_policy(
    ui: &Ui,
    decision: &webkit6::PolicyDecision,
    kind: webkit6::PolicyDecisionType,
) -> bool {
    let nav = match kind {
        webkit6::PolicyDecisionType::NavigationAction
        | webkit6::PolicyDecisionType::NewWindowAction => decision
            .downcast_ref::<webkit6::NavigationPolicyDecision>()
            .and_then(|d| d.navigation_action())
            .and_then(|a| a.request())
            .and_then(|r| r.uri()),
        _ => return false,
    };
    let Some(uri) = nav else {
        decision.ignore();
        return true;
    };
    let view_dir = ui.state.borrow().cache.join("view");
    if uri.starts_with("file://")
        && gio::File::for_uri(&uri)
            .path()
            .is_some_and(|p| p.starts_with(&view_dir))
        && kind == webkit6::PolicyDecisionType::NavigationAction
    {
        return false; // default handling: load it
    }
    decision.ignore();
    match Link::parse(&uri) {
        Some(Link::External(url)) => {
            gtk::UriLauncher::new(&url).launch(Some(&ui.window), gio::Cancellable::NONE, |_| {});
        }
        Some(link) => {
            let current = ui.state.borrow().current.clone();
            let resolved = navigate::resolve(&ui.state.borrow().library, current.as_ref(), &link);
            match resolved {
                Ok(Some(target)) => open(ui, &target),
                Ok(None) => not_in_library(ui, &link),
                Err(e) => toast(ui, &format!("Cannot follow link: {e}")),
            }
        }
        None => {}
    }
    true
}

fn not_in_library(ui: &Ui, link: &Link) {
    let toast = match link {
        Link::Document {
            lang_code,
            meps_document_id,
        } => {
            let t = adw::Toast::builder()
                .title("This publication is not in your library")
                .button_label("Open on jw.org")
                .build();
            let url =
                format!("https://www.jw.org/finder?wtlocale={lang_code}&docid={meps_document_id}");
            let window = ui.window.clone();
            t.connect_button_clicked(move |_| {
                gtk::UriLauncher::new(&url).launch(Some(&window), gio::Cancellable::NONE, |_| {});
            });
            t
        }
        _ => adw::Toast::new("No Bible in your library"),
    };
    ui.toasts.add_toast(toast);
}

/// Open a publication (symbol or directory name) and optionally a document
/// or `BOOK:CHAPTER`, as given on the command line.
fn open_initial(ui: &Ui, publication: &str, doc: Option<&str>) {
    let entry = {
        let lib = &ui.state.borrow().library;
        match lib.get_by_dir(publication) {
            Ok(Some(e)) => Some(e),
            _ => lib
                .find(publication, None, None)
                .ok()
                .and_then(|v| v.into_iter().next()),
        }
    };
    let Some(entry) = entry else {
        toast(ui, &format!("{publication} is not in the library"));
        return;
    };
    show_publication(ui, &entry);
    let kind = match doc.map(|d| (d, d.split_once(':'))) {
        Some((_, Some((b, c)))) => match (b.parse(), c.parse()) {
            (Ok(book), Ok(chapter)) => TargetKind::Chapter {
                book,
                chapter,
                verse: 1,
            },
            _ => return toast(ui, "expected BOOK:CHAPTER"),
        },
        Some((d, None)) => match d.parse() {
            Ok(id) => TargetKind::Document(id),
            Err(_) => return toast(ui, "expected a document number"),
        },
        None => return,
    };
    open(
        ui,
        &Target {
            publication: entry.dir_name,
            kind,
        },
    );
}
