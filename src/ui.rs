use crate::{
    REPOSITORY, VERSION,
    command::UninstallPlan,
    i18n::{Language, Message},
    inventory::{InstalledApp, Inventory},
    leftovers::{Candidate, QuarantineRecord, Snapshot},
    removal::{self, Phase, Status},
    settings::{Settings, Theme},
    updater::Update,
    worker::{ElevationOffer, Event, Work, Worker},
};
use egui::{self, Color32, RichText};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Applications,
    Leftovers,
    Quarantine,
    Settings,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Publisher,
    Size,
}
struct Confirmation {
    app: InstalledApp,
    plan: UninstallPlan,
    snapshot: Option<Snapshot>,
    limitation: Option<String>,
    accepted: bool,
}
struct RemovalView {
    app_id: String,
    status: Status,
    elevation: Option<ElevationOffer>,
}
pub struct Desktop {
    worker: Option<Worker>,
    page: Page,
    apps: Vec<InstalledApp>,
    icons: HashMap<String, egui::TextureHandle>,
    inventory_warnings: Vec<String>,
    warnings: Vec<String>,
    selected: Option<String>,
    query: String,
    sort: Sort,
    show_system: bool,
    visible: Vec<usize>,
    filter_dirty: bool,
    busy: bool,
    message: Message,
    error: Option<String>,
    confirmation: Option<Confirmation>,
    snapshot: Option<Snapshot>,
    candidate: Option<Candidate>,
    candidate_selected: bool,
    restore_confirm: Option<QuarantineRecord>,
    records: Vec<QuarantineRecord>,
    settings: Settings,
    applied_theme: Theme,
    data: Option<PathBuf>,
    update: Option<Update>,
    update_status: Message,
    update_busy: bool,
    progress: Option<(u64, u64)>,
    staged: Option<crate::updater::StagedUpdate>,
    saved_settings: Option<Settings>,
    exit_requested: bool,
    removal_view: Option<RemovalView>,
    removed_rows: HashMap<String, Instant>,
    preview_now: Option<Instant>,
}
impl Desktop {
    /// Isolated real UI with deterministic data and no worker or external effects.
    pub fn preview(ctx: &egui::Context, screen: &str) -> anyhow::Result<Self> {
        let mut app = Self::create(ctx, None);
        app.page = match screen {
            "applications"
            | "applications-load"
            | "applications-unavailable"
            | "applications-administrator"
            | "applications-uac-cancelled"
            | "applications-removing"
            | "applications-removed"
            | "applications-collapsing"
            | "applications-removal-load" => Page::Applications,
            "settings" => Page::Settings,
            "leftovers" => Page::Leftovers,
            "quarantine" => Page::Quarantine,
            _ => anyhow::bail!("Unknown screen: {screen}"),
        };
        app.busy = false;
        app.data = Some(PathBuf::from(
            r"C:\Users\Example\AppData\Local\AppUninstaller",
        ));
        app.message = "Isolated preview · sample data · removal and update actions disabled".into();
        app.update_status = "Updates have not been checked in this isolated preview.".into();
        for (index, (name, publisher, version, kib)) in [
            ("Atlas Notes", "Atlas Software", "3.4.1", 82432),
            ("Canvas Studio", "Canvas Tools", "2.8.0", 428032),
            ("Focus Timer", "Independent Studio", "1.6.2", 18432),
            ("Northwind Archive", "Northwind", "5.2.0", 102400),
            ("Orbit Player", "Orbit Project", "4.0.2", 145408),
            ("Paperwork", "Paperwork Contributors", "0.9.8", 53248),
        ]
        .into_iter()
        .enumerate()
        {
            let root = PathBuf::from(format!(r"C:\Program Files\{name}"));
            app.apps.push(InstalledApp {
                id: format!("preview/{index}"),
                key_name: name.into(),
                scope: "All users".into(),
                view: "64-bit".into(),
                name: name.into(),
                publisher: publisher.into(),
                version: version.into(),
                estimated_kib: Some(kib),
                install_location: Some(root.clone()),
                uninstall_string: format!("\"{}\\uninstall.exe\"", root.display()),
                ..Default::default()
            });
        }
        app.selected = Some(app.apps[0].id.clone());
        if screen == "applications-load" || screen == "applications-removal-load" {
            for index in 6..840 {
                let mut entry = app.apps[index % 6].clone();
                entry.id = format!("preview/{index}");
                entry.name = format!("{} {index:03}", entry.name);
                entry.system_component = index % 4 != 0;
                app.apps.push(entry);
            }
        }
        if matches!(
            screen,
            "applications-unavailable"
                | "applications-administrator"
                | "applications-uac-cancelled"
                | "applications-removing"
                | "applications-removed"
                | "applications-collapsing"
                | "applications-removal-load"
        ) {
            let now = Instant::now();
            app.preview_now = Some(now);
            let needs_administrator = matches!(
                screen,
                "applications-administrator" | "applications-uac-cancelled"
            );
            let phase = if screen == "applications-unavailable" || needs_administrator {
                Phase::Waiting
            } else if screen == "applications-removing" {
                Phase::Running
            } else {
                Phase::Removed
            };
            let update = crate::worker::RemovalUpdate {
                app: app.apps[0].clone(),
                status: Status {
                    phase,
                    process_running: phase == Phase::Running,
                    exit_code: if phase == Phase::Removed {
                        Some(0)
                    } else {
                        None
                    },
                    monitoring: true,
                },
                problem: if screen == "applications-unavailable" {
                    Some("Nothing was launched: the registered uninstaller file is missing.".into())
                } else {
                    None
                },
                snapshot: None,
                elevation: if needs_administrator {
                    Some(ElevationOffer {
                        plan: crate::command::plan(&app.apps[0], r"C:\Windows")?,
                        cancelled: screen == "applications-uac-cancelled",
                    })
                } else {
                    None
                },
            };
            app.apply_removal(update, now);
            let age = match screen {
                "applications-collapsing" => Duration::from_millis(10130),
                "applications-removal-load" => Duration::from_secs(9),
                _ => Duration::from_secs(3),
            };
            if phase == Phase::Removed {
                app.removed_rows.insert("preview/0".into(), now - age);
            }
        }
        if app.page == Page::Leftovers {
            use crate::leftovers::{Candidate, Fingerprint, Snapshot};
            let files = [
                (
                    PathBuf::from("preferences.json"),
                    Fingerprint {
                        bytes: 2048,
                        sha256: "34e2a1".repeat(10) + "abcd",
                        directory: false,
                    },
                ),
                (
                    PathBuf::from("cache"),
                    Fingerprint {
                        bytes: 0,
                        sha256: String::new(),
                        directory: true,
                    },
                ),
            ]
            .into_iter()
            .collect();
            let snapshot = Snapshot {
                app_id: "preview/0".into(),
                app_name: "Atlas Notes".into(),
                root: PathBuf::from(r"C:\Program Files\Atlas Notes"),
                evidence: vec![
                    "Exact folder recorded in the uninstall registration before removal.".into(),
                    "Folder name matches the app and its registered uninstaller was inside it."
                        .into(),
                    "No other app registration uses this folder.".into(),
                    "Remaining files match their original SHA-256 fingerprints.".into(),
                ],
                files,
            };
            app.candidate = Some(Candidate {
                files: snapshot.files.clone(),
                snapshot,
            });
        }
        Ok(app)
    }
    pub fn new(ctx: &egui::Context) -> Self {
        Self::create(ctx, Some(Worker::start(ctx.clone())))
    }
    fn create(ctx: &egui::Context, worker: Option<Worker>) -> Self {
        crate::design::configure(ctx);
        Self {
            worker,
            page: Page::Applications,
            apps: vec![],
            icons: HashMap::new(),
            inventory_warnings: vec![],
            warnings: vec![],
            selected: None,
            query: String::new(),
            sort: Sort::Name,
            show_system: false,
            visible: vec![],
            filter_dirty: true,
            busy: true,
            message: "Reading uninstall registrations…".into(),
            error: None,
            confirmation: None,
            snapshot: None,
            candidate: None,
            candidate_selected: false,
            restore_confirm: None,
            records: vec![],
            settings: Settings::default(),
            applied_theme: Theme::default(),
            data: None,
            update: None,
            update_status: "No check completed in this session.".into(),
            update_busy: false,
            progress: None,
            staged: None,
            saved_settings: None,
            exit_requested: false,
            removal_view: None,
            removed_rows: HashMap::new(),
            preview_now: None,
        }
    }
    pub fn use_live_preview_clock(&mut self) {
        self.preview_now = None;
    }
    fn apply_removal(&mut self, update: crate::worker::RemovalUpdate, now: Instant) {
        let crate::worker::RemovalUpdate {
            app,
            status,
            problem,
            snapshot,
            elevation,
        } = update;
        let removed = status.phase == Phase::Removed;
        let new_session = self
            .removal_view
            .as_ref()
            .is_none_or(|view| view.app_id != app.id);
        if new_session || !removed {
            self.snapshot = None;
        }
        if removed {
            self.removed_rows.entry(app.id.clone()).or_insert(now);
        } else {
            self.removed_rows.remove(&app.id);
        }
        let expired = self
            .removed_rows
            .get(&app.id)
            .is_some_and(|at| removal::row_progress(now.saturating_duration_since(*at)).finished);
        if !self.apps.iter().any(|entry| entry.id == app.id) && !expired {
            self.apps.push(app.clone());
        }
        self.filter_dirty = true;
        self.busy = false;
        self.confirmation = None;
        self.error = if removed { None } else { problem.clone() };
        if let Some(snapshot) = snapshot {
            self.snapshot = Some(snapshot);
        }
        if status
            .exit_code
            .is_some_and(|code| [3010, 1641].contains(&code))
        {
            self.snapshot = None;
        }
        let (key, values) = match status.phase {
            Phase::Waiting if elevation.as_ref().is_some_and(|offer| offer.cancelled) => (
                "{0}: administrator request canceled. Nothing was launched.",
                vec![app.name.clone()],
            ),
            Phase::Waiting if elevation.is_some() => (
                "{0}: administrator permission is needed. Nothing was launched.",
                vec![app.name.clone()],
            ),
            Phase::Running => (
                "{0}: publisher uninstaller is running. Status is checked automatically.",
                vec![app.name.clone()],
            ),
            Phase::Waiting if problem.is_some() => (
                "{0}: watching its Windows registration. Review the issue above; removal is not confirmed.",
                vec![app.name.clone()],
            ),
            Phase::Waiting if status.exit_code.is_some() => (
                "{0}: waiting for Windows to confirm removal (publisher exit code {1}).",
                vec![app.name.clone(), status.exit_code.unwrap().to_string()],
            ),
            Phase::Waiting => (
                "{0}: waiting for Windows to confirm removal.",
                vec![app.name.clone()],
            ),
            Phase::VerificationUnavailable => (
                "{0}: removal cannot be verified. The item is kept in the list.",
                vec![app.name.clone()],
            ),
            Phase::Unconfirmed => (
                "{0}: observation stopped without confirmed removal. Review the registered action again or refresh.",
                vec![app.name.clone()],
            ),
            Phase::Removed if status.process_running => (
                "{0} is no longer listed in Windows. Its publisher uninstaller is still running.",
                vec![app.name.clone()],
            ),
            Phase::Removed
                if status
                    .exit_code
                    .is_some_and(|code| [3010, 1641].contains(&code)) =>
            {
                (
                    "{0} is no longer listed in Windows. Restart required; leftover review is disabled.",
                    vec![app.name.clone()],
                )
            }
            Phase::Removed => (
                "{0} removed from the installed-app list. Leftover review remains a separate action.",
                vec![app.name.clone()],
            ),
        };
        self.message = Message::new(key, values);
        self.removal_view = Some(RemovalView {
            app_id: app.id,
            status,
            elevation,
        });
    }
    fn update_removed_rows(&mut self, ctx: &egui::Context) {
        let now = self.preview_now.unwrap_or_else(Instant::now);
        let mut expired = vec![];
        let mut wake = None;
        for app in &self.apps {
            if let Some(at) = self.removed_rows.get(&app.id) {
                let progress = removal::row_progress(now.saturating_duration_since(*at));
                if progress.finished {
                    expired.push(app.id.clone());
                } else if self.page == Page::Applications {
                    wake = Some(wake.map_or(progress.next_wake, |old: Duration| {
                        old.min(progress.next_wake)
                    }));
                }
            }
        }
        if !expired.is_empty() {
            self.apps.retain(|app| !expired.contains(&app.id));
            if self
                .selected
                .as_ref()
                .is_some_and(|id| expired.contains(id))
            {
                self.selected = None;
            }
            self.filter_dirty = true;
        }
        if let Some(delay) = wake {
            ctx.request_repaint_after(delay);
        }
    }
    fn send(&mut self, work: Work, message: &'static str) {
        let result = self.worker.as_ref().map_or_else(
            || Err("Actions are disabled in an isolated preview.".into()),
            |worker| worker.send(work),
        );
        match result {
            Ok(()) => {
                self.busy = true;
                self.error = None;
                self.message = message.into();
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn save_preferences(&mut self, previous: Settings) {
        let result = self.worker.as_ref().map_or_else(
            || Err("Actions are disabled in an isolated preview.".into()),
            |worker| worker.send(Work::Save(self.settings.clone())),
        );
        match result {
            Ok(()) => {
                self.saved_settings = Some(previous);
                self.busy = true;
                self.error = None;
                self.message = "Saving settings…".into();
            }
            Err(error) => {
                self.settings = previous;
                self.error = Some(error);
            }
        }
    }
    fn set_inventory(&mut self, inventory: Inventory, ctx: &egui::Context) {
        self.icons.clear();
        for app in &inventory.apps {
            if let Some(icon) = &app.icon {
                self.icons.insert(
                    app.id.clone(),
                    ctx.load_texture(
                        &app.id,
                        egui::ColorImage::from_rgba_unmultiplied(
                            [icon.width, icon.height],
                            &icon.rgba,
                        ),
                        egui::TextureOptions::LINEAR,
                    ),
                );
            }
        }
        let mut incoming = inventory.apps;
        if inventory.warnings.is_empty() {
            for app in &incoming {
                if self.removed_rows.remove(&app.id).is_some() {
                    if let Some(view) = &mut self.removal_view
                        && view.app_id == app.id
                    {
                        if view.status.monitoring {
                            view.status.phase = Phase::Waiting;
                        } else {
                            self.removal_view = None;
                        }
                    }
                    self.snapshot = None;
                    self.message = Message::new(
                        "{0} is registered again; it is kept in the list.",
                        vec![app.name.clone()],
                    );
                }
            }
        }
        let now = self.preview_now.unwrap_or_else(Instant::now);
        for old in &self.apps {
            let grace = self.removed_rows.get(&old.id).is_some_and(|at| {
                !removal::row_progress(now.saturating_duration_since(*at)).finished
            });
            let watching = self.removal_view.as_ref().is_some_and(|view| {
                view.app_id == old.id
                    && view.status.monitoring
                    && !self.removed_rows.contains_key(&old.id)
            });
            if (grace || watching) && !incoming.iter().any(|app| app.id == old.id) {
                incoming.push(old.clone());
            }
        }
        self.apps = incoming;
        self.inventory_warnings = inventory.warnings;
        if !self
            .apps
            .iter()
            .any(|a| self.selected.as_ref() == Some(&a.id))
        {
            self.selected = None;
        }
        self.filter_dirty = true;
    }
    pub fn receive_background(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        while let Some(event) = self
            .worker
            .as_ref()
            .and_then(|worker| worker.rx.try_recv().ok())
        {
            changed = true;
            match event {
                Event::Ready {
                    settings,
                    data,
                    inventory,
                    records,
                    warnings,
                } => {
                    self.settings = settings;
                    self.data = data;
                    self.records = records;
                    self.warnings = warnings;
                    self.set_inventory(inventory, ctx);
                    self.busy = false;
                    self.message =
                        "Choose an application to inspect its registered removal action.".into();
                }
                Event::Inventory(inventory) => {
                    self.set_inventory(inventory, ctx);
                    self.busy = false;
                    self.message = "Inventory refreshed.".into();
                }
                Event::Prepared {
                    app,
                    plan,
                    snapshot,
                    limitation,
                } => {
                    self.confirmation = Some(Confirmation {
                        app: *app,
                        plan,
                        snapshot,
                        limitation,
                        accepted: false,
                    });
                    self.busy = false;
                    self.message =
                        "Review the registered command before launching the uninstaller.".into();
                }
                Event::Removal(update) => {
                    self.apply_removal(*update, self.preview_now.unwrap_or_else(Instant::now))
                }
                Event::Reviewed(candidate, inventory) => {
                    self.set_inventory(inventory, ctx);
                    self.message = if candidate.is_some() {
                        "Review every path and explanation, then explicitly select the folder."
                            .into()
                    } else {
                        "No remaining installation folder was found.".into()
                    };
                    self.candidate = candidate;
                    self.candidate_selected = false;
                    self.busy = false;
                }
                Event::Quarantined(record) => {
                    self.message = Message::new(
                        "{0} moved into reversible quarantine. Disk space has not been reclaimed.",
                        vec![record.app_name.clone()],
                    );
                    self.records.push(record);
                    self.candidate = None;
                    self.snapshot = None;
                    self.busy = false;
                    self.page = Page::Quarantine;
                }
                Event::Restored(id) => {
                    self.records.retain(|r| r.id != id);
                    self.busy = false;
                    self.message =
                        "Original folder restored. This does not reinstall the application.".into();
                }
                Event::Saved => {
                    self.saved_settings = None;
                    self.busy = false;
                    self.message = "Settings saved.".into();
                }
                Event::UpdateStarted => {
                    self.update_busy = true;
                    self.update_status = "Checking the public stable release…".into();
                }
                Event::UpdateChecked(result) => {
                    self.update_busy = false;
                    // A startup check does not own the UI's general operation state.
                    if self.message == "Checking for updates…" {
                        self.busy = false;
                    }
                    match result {
                        Ok(Some(update)) => {
                            self.update_status = Message::new(
                                "Version {0} is available.",
                                vec![update.version.to_string()],
                            );
                            self.update = Some(update);
                        }
                        Ok(None) => {
                            self.update_status =
                                "You are up to date for the stable channel.".into();
                            self.update = None;
                        }
                        Err(e) => {
                            self.update_status =
                                Message::new("Could not check for updates: {0}", vec![e]);
                        }
                    }
                }
                Event::Progress(done, total) => {
                    self.progress = Some((done, total));
                }
                Event::Downloaded(path) => {
                    self.staged = Some(path);
                    self.progress = None;
                    self.busy = false;
                    self.message="Update downloaded and SHA-256 verified. Restart to install when you are ready.".into();
                }
                Event::Installing => {
                    self.exit_requested = true;
                }
                Event::Error(error) => {
                    self.busy = false;
                    self.progress = None;
                    self.error = Some(error);
                    self.message = "Action could not be completed. Review the error above.".into();
                    self.update_busy = false;
                    if let Some(settings) = self.saved_settings.take() {
                        self.settings = settings;
                    }
                }
            }
        }
        changed
    }
    pub fn exit_requested(&self) -> bool {
        self.exit_requested
    }
    fn rebuild_filter(&mut self) {
        if !self.filter_dirty {
            return;
        }
        self.visible = self
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| {
                (self.show_system || !app.system_component) && app.matches(&self.query)
            })
            .map(|(i, _)| i)
            .collect();
        match self.sort {
            Sort::Name => self
                .visible
                .sort_by_cached_key(|i| self.apps[*i].name.to_lowercase()),
            Sort::Publisher => self.visible.sort_by_cached_key(|i| {
                (
                    self.apps[*i].publisher.to_lowercase(),
                    self.apps[*i].name.to_lowercase(),
                )
            }),
            Sort::Size => self
                .visible
                .sort_by_key(|i| std::cmp::Reverse(self.apps[*i].estimated_kib.unwrap_or(0))),
        }
        self.filter_dirty = false;
    }
    fn applications(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        let palette = crate::design::palette(self.settings.theme);
        let l = self.settings.language;
        ui.heading(l.text("Installed applications"));
        ui.label(
            RichText::new(l.text(
                "Choose an app and review its registered uninstaller before making changes.",
            ))
            .color(palette.muted),
        );
        ui.add_space(10.);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(l.text("Search by name or publisher"))
                        .desired_width(300.),
                )
                .changed()
            {
                self.filter_dirty = true;
            }
            egui::ComboBox::from_id_salt("sort")
                .selected_text(match self.sort {
                    Sort::Name => l.text("Name"),
                    Sort::Publisher => l.text("Publisher"),
                    Sort::Size => l.text("Largest first"),
                })
                .show_ui(ui, |ui| {
                    for (sort, label) in [
                        (Sort::Name, l.text("Name")),
                        (Sort::Publisher, l.text("Publisher")),
                        (Sort::Size, l.text("Largest first")),
                    ] {
                        if ui.selectable_value(&mut self.sort, sort, label).changed() {
                            self.filter_dirty = true;
                        }
                    }
                });
            if ui
                .add_enabled(!self.busy, egui::Button::new(l.text("Refresh")))
                .clicked()
            {
                self.send(Work::Refresh, "Refreshing installed applications…");
            }
        });
        ui.horizontal(|ui| {
            if ui
                .checkbox(
                    &mut self.show_system,
                    l.text("Include system components (view only)"),
                )
                .changed()
            {
                self.filter_dirty = true;
            }
            self.rebuild_filter();
            ui.label(
                RichText::new({
                    let removed = self
                        .visible
                        .iter()
                        .filter(|index| self.removed_rows.contains_key(&self.apps[**index].id))
                        .count();
                    if removed > 0 {
                        l.format(
                            "{0} installed · {1} removed",
                            &[
                                (self.visible.len() - removed).to_string(),
                                removed.to_string(),
                            ],
                        )
                    } else {
                        l.format("{0} apps shown", &[self.visible.len().to_string()])
                    }
                })
                .small()
                .color(palette.muted),
            );
        });
        ui.add_space(6.);
        if !self.inventory_warnings.is_empty() {
            ui.colored_label(ui.visuals().warn_fg_color,l.text("Inventory is incomplete. Review the read errors in Settings before removing an app."));
        }
        ui.columns(2,|columns| {
            egui::ScrollArea::vertical().auto_shrink([false,false]).id_salt("inventory").show_viewport(&mut columns[0],|ui,viewport| {
                let now=self.preview_now.unwrap_or_else(Instant::now);
                let origin=ui.max_rect().min;let width=ui.available_width();let mut y=0.;let mut rows=vec![];
                let gap=ui.spacing().item_spacing.y;
                for index in &self.visible {
                    let app=&self.apps[*index];
                    let progress=self.removed_rows.get(&app.id).map(|at|removal::row_progress(now.saturating_duration_since(*at)));
                    let fraction=progress.map_or(1.,|progress|progress.fraction);
                    let height=70.*fraction;
                    if height>0. && y+height>=viewport.min.y && y<=viewport.max.y {rows.push((*index,y,height,progress));}
                    y+=(70.+gap)*fraction;
                }
                ui.set_min_height(y);
                for (index,y,height,progress) in rows {
                    let app=&self.apps[index];let selected=self.selected.as_ref()==Some(&app.id);
                    let rect=egui::Rect::from_min_size(origin+egui::vec2(0.,y),egui::vec2(width,height));
                    let mut row_ui=ui.new_child(egui::UiBuilder::new().id_salt(&app.id).max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
                    row_ui.set_clip_rect(row_ui.clip_rect().intersect(rect));
                    if let Some(progress)=progress {row_ui.multiply_opacity(progress.fraction);}
                    let response=egui::Frame::new().fill(if selected {palette.selection} else {Color32::TRANSPARENT}).inner_margin(10.).show(&mut row_ui,|ui| {
                        ui.set_width((width-20.).max(1.));
                        ui.horizontal(|ui| {
                            if let Some(icon)=self.icons.get(&app.id) {ui.image((icon.id(),egui::vec2(30.,30.)));}
                            ui.vertical(|ui| {
                                ui.spacing_mut().button_padding=egui::vec2(0.,2.);
                                let mut title=RichText::new(&app.name).strong();if progress.is_some(){title=title.strikethrough().color(palette.muted);}
                                if ui.add(egui::Button::new(title).frame(false).truncate()).clicked(){self.selected=Some(app.id.clone());}
                                let label=if let Some(progress)=progress {
                                    if progress.seconds_left>0 {l.format("Removed · leaves this list in {0}s", &[progress.seconds_left.to_string()])}else{l.text("Removed").to_string()}
                                }else if let Some(view)=&self.removal_view && view.app_id==app.id && view.elevation.is_some() {
                                    if view.elevation.as_ref().is_some_and(|offer|offer.cancelled){l.text("Administrator request canceled").into()}else{l.text("Administrator permission needed").into()}
                                }else if let Some(view)=&self.removal_view && view.app_id==app.id {
                                    match view.status.phase {Phase::Running=>l.text("Uninstalling…").into(),Phase::Waiting=>l.text("Checking Windows registration…").into(),Phase::VerificationUnavailable=>l.text("Removal cannot be verified").into(),Phase::Unconfirmed=>l.text("Removal not confirmed").into(),Phase::Removed=>l.text("Removed").to_string()}
                                }else{format!("{} · {}",if app.publisher.is_empty(){l.text("Unknown publisher")}else{&app.publisher},size(app.estimated_kib.map(|n|n*1024), l))};
                                ui.add(egui::Label::new(RichText::new(label).small().color(if progress.is_some(){palette.accent_text}else{palette.muted})).truncate());
                            });
                        });
                    });
                    if response.response.interact(egui::Sense::click()).clicked(){self.selected=Some(app.id.clone());}
                }
            });
            egui::ScrollArea::vertical().auto_shrink([false,false]).id_salt("app-details").show(&mut columns[1],|ui| {
                egui::Frame::new().fill(palette.surface).inner_margin(20.).show(ui,|ui| {
                    ui.set_width((ui.available_width()).max(1.));
                    let app=self.apps.iter().find(|a|self.selected.as_ref()==Some(&a.id)).cloned();
                    if let Some(app)=app {
                        ui.heading(&app.name);
                        ui.label(RichText::new(if app.publisher.is_empty(){l.text("Publisher not supplied")}else{&app.publisher}).color(palette.muted));
                        ui.add_space(8.);
                        let elevation=self.removal_view.as_ref().filter(|view|view.app_id==app.id).and_then(|view|view.elevation.clone());
                        if let Some(offer)=&elevation {
                            ui.label(RichText::new(if offer.cancelled{l.text("Administrator request canceled")}else{l.text("Administrator permission needed")}).color(palette.accent_text).strong());
                            ui.label(RichText::new(l.text("Run the reviewed uninstaller with administrator permission. Windows will ask for approval; AppUninstaller keeps its current permissions.")).small().color(palette.muted));
                            ui.horizontal(|ui| {
                                if ui.add_enabled(!self.busy,egui::Button::new(RichText::new(l.text("Run as administrator…")).color(Color32::WHITE)).fill(palette.accent)).clicked(){self.send(Work::RetryAsAdministrator(app.id.clone()), "Requesting administrator permission in Windows…");}
                            });
                        }else if let Some(at)=self.removed_rows.get(&app.id) {
                            let progress=removal::row_progress(self.preview_now.unwrap_or_else(Instant::now).saturating_duration_since(*at));
                            let label=if progress.seconds_left>0 {l.format("Removed from installed apps · leaves this list in {0}s", &[progress.seconds_left.to_string()])}else{l.text("Removed from installed apps").into()};
                            ui.label(RichText::new(label).color(palette.accent_text).strong());
                            if self.snapshot.is_some() && ui.button(l.text("Review leftovers")).clicked(){self.page=Page::Leftovers;}
                        }else if let Some(view)=&self.removal_view && view.app_id==app.id && view.status.monitoring {
                            let (title,help)=match view.status.phase {
                                Phase::Running=>(l.text("Uninstalling…"),l.text("Finish any publisher windows. The installed-app registration is checked automatically.")),
                                Phase::VerificationUnavailable=>(l.text("Unable to verify removal"),l.text("The item stays in the list until its registration can be checked.")),
                                _=>(l.text("Checking removal…"),l.text("Windows still lists this app. Watching for its registration to disappear.")),
                            };
                            ui.label(RichText::new(title).color(palette.accent_text).strong());
                            ui.label(RichText::new(help).small().color(palette.muted));
                        }else{
                            ui.horizontal(|ui| {
                                let watching=self.removal_view.as_ref().is_some_and(|view|view.status.monitoring);
                                if ui.add_enabled(!self.busy&&!watching&&!app.system_component&&self.inventory_warnings.is_empty(),egui::Button::new(RichText::new(l.text("Review uninstall…")).color(Color32::WHITE)).fill(palette.accent)).clicked(){self.send(Work::Prepare(app.clone()), "Validating registration and capturing bounded ownership evidence…");}
                            });
                            ui.label(RichText::new(l.text("Nothing changes until you confirm the registered action.")).small().color(palette.muted));
                        }
                        ui.add_space(12.);
                        let value_width=(ui.available_width()-116.).max(90.);
                        egui::Grid::new("metadata").num_columns(2).min_col_width(96.).max_col_width(value_width).spacing([16.,8.]).show(ui,|ui| {
                            for (label,value) in [(l.text("Version"),app.version.clone()),(l.text("Size"),size(app.estimated_kib.map(|n|n*1024), l)),(l.text("Installed for"),format!("{} · {}",l.text(&app.scope),app.view)),(l.text("Folder"),app.install_location.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||l.text("Not supplied").into()))] {
                                ui.add(egui::Label::new(RichText::new(label).color(palette.muted)).wrap_mode(egui::TextWrapMode::Extend));
                                ui.add(egui::Label::new(if value.is_empty(){l.text("Not supplied")}else{&value}).halign(egui::Align::Min).wrap());ui.end_row();
                            }
                        });
                        ui.add_space(16.);ui.strong(if elevation.is_some(){l.text("Reviewed uninstall command")}else{l.text("Registered uninstall command")});
                        egui::Frame::new().fill(palette.background).inner_margin(10.).show(ui,|ui| {
                            let command=elevation.as_ref().map(|offer|format!("\"{}\" {}",offer.plan.executable.display(),offer.plan.arguments));
                            ui.add(egui::Label::new(RichText::new(command.as_deref().unwrap_or(if app.uninstall_string.is_empty(){l.text("Not supplied")}else{&app.uninstall_string})).monospace()).halign(egui::Align::Min).selectable(true).wrap());
                        });
                        ui.add_space(10.);
                        ui.collapsing(l.text("How removal and recovery work"),|ui| {
                            ui.label(l.text("After uninstalling, review proven leftovers and choose whether to quarantine them. Quarantine is reversible; the publisher’s uninstall is not."));
                            ui.small(l.text("Uncertain ownership and unsupported command formats are refused. If the uninstaller requires administrator permission, an explicit retry opens Windows UAC. No automatic elevation or registry cleaning."));
                        });
                    } else {
                        ui.heading(l.text("Select an application"));
                        ui.label(l.text("See its publisher, installation folder and registered removal action here."));
                        ui.add_space(12.);
                        ui.label(l.text("Review → Uninstall → Inspect leftovers → Quarantine if needed"));
                    }
                });
            });
        });
    }
    fn leftovers(&mut self, ui: &mut egui::Ui) {
        let l = self.settings.language;
        let mut action = None;
        ui.heading(l.text("Explainable leftover review"));
        ui.label(l.text("Only a proven installation folder can be suggested. Documents, shared locations, custom paths and registry values are excluded. Missing ownership evidence means no cleanup suggestion."));
        if let Some(snapshot) = &self.snapshot {
            ui.add_space(10.);
            ui.strong(&snapshot.app_name);
            ui.monospace(snapshot.root.display().to_string());
            if ui
                .add_enabled(
                    !self.busy,
                    egui::Button::new(l.text("Publisher uninstaller finished · Review leftovers")),
                )
                .clicked()
            {
                action = Some((
                    Work::Review(snapshot.clone()),
                    "Checking registration removal and unchanged ownership evidence…",
                ));
            }
        } else if self.candidate.is_none() {
            ui.add_space(15.);
            ui.label(l.text("No removal session has evidence to review. Start with Review uninstall in Applications. Evidence is kept in memory for this session only."));
        }
        if let Some(candidate) = &self.candidate {
            ui.separator();
            ui.heading(l.text("Folder preview"));
            ui.monospace(candidate.snapshot.root.display().to_string());
            let bytes = candidate.files.values().map(|f| f.bytes).sum();
            ui.label(l.format(
                "{0} unchanged files and folders · {1}",
                &[candidate.files.len().to_string(), size(Some(bytes), l)],
            ));
            for evidence in &candidate.snapshot.evidence {
                ui.label(format!("• {}", l.text(evidence)));
            }
            ui.collapsing(
                l.text("Every proposed file/folder and file SHA-256"),
                |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(200.)
                        .show(ui, |ui| {
                            for (path, fingerprint) in &candidate.files {
                                ui.monospace(path.display().to_string());
                                if fingerprint.directory {
                                    ui.small(l.text("Folder"));
                                } else {
                                    ui.small(format!(
                                        "{} · {}",
                                        size(Some(fingerprint.bytes), l),
                                        fingerprint.sha256
                                    ));
                                }
                            }
                            if candidate.files.is_empty() {
                                ui.label(l.text("Only the empty folder remains."));
                            }
                        });
                },
            );
            ui.add_space(10.);
            ui.checkbox(
                &mut self.candidate_selected,
                l.text("I reviewed this folder and select it for reversible quarantine"),
            );
            ui.label(l.text("Quarantine renames the folder beside its original location. It preserves data and uses the same disk space. Restore is available in Quarantine. Access errors leave the original in place."));
            if ui
                .add_enabled(
                    !self.busy && self.candidate_selected && self.data.is_some(),
                    egui::Button::new(l.text("Quarantine selected folder")),
                )
                .clicked()
            {
                action = Some((
                    Work::Quarantine(candidate.clone()),
                    "Revalidating the preview and moving the selected folder to quarantine…",
                ));
            }
        }
        if let Some((work, message)) = action {
            self.send(work, message);
        }
    }
    fn quarantine(&mut self, ui: &mut egui::Ui) {
        let l = self.settings.language;
        ui.heading(l.text("Recoverable quarantine"));
        ui.label(l.text("Quarantined folders remain on the same drive. Restore never overwrites an existing location. Restoring files does not reinstall a program. Permanent deletion is intentionally manual."));
        if self.records.is_empty() {
            ui.add_space(15.);
            ui.label(l.text("No quarantined folders."));
        }
        for record in &self.records {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong(&record.app_name);
                ui.label(l.format("Original: {0}", &[record.original.display().to_string()]));
                ui.label(l.format("Quarantine: {0}", &[record.backup.display().to_string()]));
                ui.small(l.format(
                    "{0} files and folders · journal {1}",
                    &[record.files.len().to_string(), record.id.clone()],
                ));
                if ui
                    .add_enabled(!self.busy, egui::Button::new(l.text("Review restore…")))
                    .clicked()
                {
                    self.restore_confirm = Some(record.clone());
                }
            });
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui) {
        let l = self.settings.language;
        ui.set_max_width(760.);
        let watching = self
            .removal_view
            .as_ref()
            .is_some_and(|view| view.status.monitoring);
        ui.heading(l.text("Settings & updates"));
        ui.label(l.format(
            "AppUninstaller {0} · Windows x64 portable · MIT license",
            &[VERSION.into()],
        ));
        ui.add_space(10.);
        let previous_display = self.settings.clone();
        ui.strong(l.text("Appearance & language"));
        ui.add_enabled_ui(!self.busy && self.data.is_some(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(l.text("Language"));
                egui::ComboBox::from_id_salt("display-language")
                    .selected_text(self.settings.language.native_name())
                    .show_ui(ui, |ui| {
                        for language in crate::i18n::LANGUAGES {
                            ui.selectable_value(
                                &mut self.settings.language,
                                language,
                                language.native_name(),
                            );
                        }
                    });
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(l.text("Theme"));
                ui.selectable_value(&mut self.settings.theme, Theme::Light, l.text("Light"));
                ui.selectable_value(&mut self.settings.theme, Theme::Dark, l.text("Dark"));
            });
        });
        ui.small(l.text("Changes apply immediately and are saved for the next launch."));
        if self.settings.language != previous_display.language
            || self.settings.theme != previous_display.theme
        {
            self.save_preferences(previous_display);
        }
        ui.separator();
        ui.strong(l.text("Updates"));
        ui.hyperlink_to(l.text("Canonical repository"), REPOSITORY);
        ui.add_space(10.);
        let previous = self.settings.clone();
        if ui
            .add_enabled(
                !self.busy && self.data.is_some(),
                egui::Checkbox::new(
                    &mut self.settings.check_on_startup,
                    l.text("Check for updates on startup"),
                ),
            )
            .changed()
        {
            self.save_preferences(previous);
        }
        ui.small(l.text("Checks quietly in the background. Turning this off prevents startup requests; manual checking stays available. Downloads and installation require your action."));
        ui.label(self.update_status.render(l));
        if watching {
            ui.small(l.text("Update operations are paused while a removal is being observed."));
        }
        if ui
            .add_enabled(
                !self.busy && !self.update_busy && !watching,
                egui::Button::new(l.text("Check for updates")),
            )
            .clicked()
        {
            self.update_busy = true;
            self.send(Work::CheckUpdate, "Checking for updates…");
        }
        if let Some(update) = self.update.clone() {
            ui.separator();
            ui.strong(l.format("Update {0}", &[update.version.to_string()]));
            ui.collapsing(l.text("Release notes"), |ui| {
                ui.label(&update.notes);
            });
            ui.label(l.text("Download is verified against the release SHA-256 before installation. The existing EXE is retained as a rollback backup. Settings and quarantine records stay in your user profile."));
            if self.staged.is_none()
                && ui
                    .add_enabled(
                        !self.busy && !watching,
                        egui::Button::new(l.text("Download and install")),
                    )
                    .clicked()
            {
                self.send(Work::Download(update), "Downloading and verifying update…");
            }
        }
        if let Some((done, total)) = self.progress {
            ui.add(
                egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(format!(
                    "{} / {}",
                    size(Some(done), l),
                    size(Some(total), l)
                )),
            );
        }
        if let Some(path) = self.staged.clone() {
            ui.label(l.text(
                "Verified download ready. Restart installs the update after this process exits.",
            ));
            if ui
                .add_enabled(
                    !self.busy && !watching,
                    egui::Button::new(l.text("Restart and install verified update")),
                )
                .clicked()
            {
                self.send(Work::Install(path), "Starting replacement helper…");
            }
        }
        ui.separator();
        ui.strong(l.text("About & local storage"));
        ui.label(l.text("CPU rendering · Windows GDI · no graphics API context"));
        if let Some(data) = &self.data {
            ui.monospace(data.display().to_string());
        } else {
            ui.label(
                l.text("User data storage unavailable. Settings and quarantine are disabled."),
            );
        }
        ui.small(l.text("No telemetry. No registry cleaning. Inventory stays local. Network access is used only for updates."));
        ui.collapsing(
            l.text("Open-source licenses and bundled font notices"),
            |ui| {
                static LINES: std::sync::LazyLock<Vec<&'static str>> =
                    std::sync::LazyLock::new(|| {
                        include_str!("../THIRD_PARTY_NOTICES.txt").lines().collect()
                    });
                egui::ScrollArea::both().max_height(260.).show_rows(
                    ui,
                    16.,
                    LINES.len(),
                    |ui, range| {
                        for row in range {
                            ui.monospace(LINES[row]);
                        }
                    },
                );
            },
        );
        for warning in self.warnings.iter().chain(self.inventory_warnings.iter()) {
            ui.colored_label(ui.visuals().warn_fg_color, l.text(warning));
        }
    }
    fn ensure_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme != self.settings.theme {
            crate::design::apply_theme(ctx, self.settings.theme);
            self.applied_theme = self.settings.theme;
        }
    }
    fn confirmations(&mut self, ctx: &egui::Context) {
        let l = self.settings.language;
        let mut launch = false;
        let mut cancel = false;
        if let Some(confirm) = &mut self.confirmation {
            egui::Window::new(l.text("Review registered uninstall action")).collapsible(false).resizable(true).default_width(640.).show(ctx,|ui| {
                ui.heading(&confirm.app.name);ui.label(l.format("Publisher: {0}", std::slice::from_ref(&confirm.app.publisher)));
                ui.strong(l.text("Executable"));ui.monospace(confirm.plan.executable.display().to_string());
                ui.strong(l.text("Arguments"));ui.monospace(&confirm.plan.arguments);ui.label(l.text(&confirm.plan.explanation));
                if let Some(limitation)=&confirm.limitation {ui.colored_label(ui.visuals().warn_fg_color,l.format("Leftover review unavailable: {0}", &[l.text(limitation).to_owned()]));}
                else {ui.label(l.text("Ownership evidence captured. A leftover suggestion still requires successful registration removal and unchanged file hashes."));}
                ui.label(l.text("This runs third-party software which may change files and settings. Save your work first. AppUninstaller cannot undo the publisher's uninstall."));
                ui.checkbox(&mut confirm.accepted,l.text("I reviewed this action and want to run the registered uninstaller"));
                ui.horizontal(|ui|{cancel=ui.button(l.text("Cancel")).clicked();launch=ui.add_enabled(confirm.accepted&&!self.busy,egui::Button::new(l.text("Run registered uninstaller"))).clicked();});
            });
        }
        if cancel {
            self.confirmation = None;
        }
        if launch && let Some(confirm) = self.confirmation.take() {
            self.snapshot = None;
            self.candidate = None;
            self.send(
                Work::Uninstall(Box::new(confirm.app), confirm.plan, confirm.snapshot),
                "Starting the registered uninstaller and checking its registration…",
            );
        }
        let mut restore = false;
        let mut dismiss = false;
        if let Some(record) = &self.restore_confirm {
            egui::Window::new(l.text("Restore quarantined folder")).collapsible(false).show(ctx,|ui| {
                ui.label(l.format("Restore {0} to:", std::slice::from_ref(&record.app_name)));ui.monospace(record.original.display().to_string());
                ui.label(l.text("Files must still match the quarantine journal. An existing original location will never be overwritten."));
                ui.horizontal(|ui|{dismiss=ui.button(l.text("Cancel")).clicked();restore=ui.add_enabled(!self.busy,egui::Button::new(l.text("Restore folder"))).clicked();});
            });
        }
        if dismiss {
            self.restore_confirm = None;
        }
        if restore && let Some(record) = self.restore_confirm.take() {
            self.send(
                Work::Restore(record),
                "Validating quarantine and restoring its original folder…",
            );
        }
    }
}
impl Desktop {
    pub fn update(&mut self, ctx: &egui::Context) {
        self.receive_background(ctx);
        self.ensure_theme(ctx);
        self.update_removed_rows(ctx);
        let palette = crate::design::palette(self.settings.theme);
        let l = self.settings.language;
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(palette.surface)
                    .inner_margin(egui::Margin::symmetric(24, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("AppUninstaller")
                            .size(24.)
                            .strong()
                            .color(palette.accent_text),
                    );
                    ui.label(
                        RichText::new(l.text("A careful way to remove apps.")).color(palette.muted),
                    );
                });

                ui.horizontal(|ui| {
                    for (page, label) in [
                        (Page::Applications, l.text("Applications")),
                        (Page::Leftovers, l.text("Leftover review")),
                        (Page::Quarantine, l.text("Quarantine")),
                        (Page::Settings, l.text("Settings")),
                    ] {
                        ui.selectable_value(&mut self.page, page, label);
                    }
                    if self.update.is_some() && ui.button(l.text("Update available")).clicked() {
                        self.page = Page::Settings;
                    }
                });
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            if let Some(error) = self.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, l.text(&error));
                    if ui.small_button(l.text("Dismiss")).clicked() {
                        self.error = None;
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                if self.busy {
                    ui.spinner();
                }
                ui.label(self.message.render(l));
                if self.removal_view.as_ref().is_some_and(|view| {
                    view.elevation.is_some()
                        && (self.page != Page::Applications
                            || self.selected.as_ref() != Some(&view.app_id))
                }) && ui
                    .small_button(l.text("Review administrator request"))
                    .clicked()
                {
                    self.selected = self.removal_view.as_ref().map(|view| view.app_id.clone());
                    self.page = Page::Applications;
                }
                if self
                    .removal_view
                    .as_ref()
                    .is_some_and(|view| view.status.monitoring)
                    && ui.small_button(l.text("Stop watching")).clicked()
                {
                    self.send(Work::StopWatching, "Stopping removal observation…");
                }
            });
        });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(24.),
            )
            .show(ctx, |ui| {
                if self.page == Page::Applications {
                    self.applications(ctx, ui);
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| match self.page {
                        Page::Leftovers => self.leftovers(ui),
                        Page::Quarantine => self.quarantine(ui),
                        Page::Settings => self.settings(ui),
                        _ => {}
                    });
                }
            });
        self.ensure_theme(ctx);
        self.confirmations(ctx);
    }
}
fn size(bytes: Option<u64>, l: Language) -> String {
    match bytes {
        None => l.text("Size not supplied").into(),
        Some(n) if n >= 1024 * 1024 * 1024 => {
            format!("{:.1} GB", n as f64 / (1024. * 1024. * 1024.))
        }
        Some(n) if n >= 1024 * 1024 => format!("{:.1} MB", n as f64 / (1024. * 1024.)),
        Some(n) => format!("{:.1} KB", n as f64 / 1024.),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_language_reformats_existing_status_without_changing_app_names() {
        let ctx = egui::Context::default();
        let mut app = Desktop::preview(&ctx, "applications-removed").unwrap();
        let english = app.message.render(Language::English);
        let russian = app.message.render(Language::Russian);
        assert_ne!(english, russian);
        assert!(english.contains("Atlas Notes"));
        assert!(russian.contains("Atlas Notes"));
        assert!(russian.contains("удалено"));
        app.settings.language = Language::French;
        assert!(
            app.message
                .render(app.settings.language)
                .contains("Atlas Notes")
        );
    }

    #[test]
    fn theme_changes_and_failed_preference_saves_restore_consistent_surfaces() {
        let ctx = egui::Context::default();
        let mut app = Desktop::preview(&ctx, "settings").unwrap();
        let previous = app.settings.clone();
        app.settings.theme = Theme::Dark;
        app.ensure_theme(&ctx);
        assert!(ctx.style().visuals.dark_mode);
        assert_eq!(
            ctx.style().visuals.window_fill,
            crate::design::palette(Theme::Dark).surface
        );
        // Capture previews cannot save. Immediate send failures must restore preferences too.
        app.save_preferences(previous);
        app.ensure_theme(&ctx);
        assert_eq!(app.settings.theme, Theme::Light);
        assert!(!ctx.style().visuals.dark_mode);
        assert!(app.error.is_some());
    }
    #[test]
    fn background_progress_and_exit_are_drained_without_building_a_frame() {
        let ctx = egui::Context::default();
        let (worker, events) = Worker::test_channel();
        let mut app = Desktop::create(&ctx, Some(worker));
        let pass = ctx.cumulative_pass_nr();
        events.send(Event::Progress(512, 1024)).unwrap();
        assert!(app.receive_background(&ctx));
        assert_eq!(app.progress, Some((512, 1024)));
        assert_eq!(ctx.cumulative_pass_nr(), pass);
        events.send(Event::Installing).unwrap();
        assert!(app.receive_background(&ctx));
        assert!(app.exit_requested());
        assert_eq!(ctx.cumulative_pass_nr(), pass);
        assert!(!app.receive_background(&ctx));
    }
    #[test]
    fn a_launch_error_replaces_the_stale_waiting_message() {
        let ctx = egui::Context::default();
        let (worker, events) = Worker::test_channel();
        let mut app = Desktop::create(&ctx, Some(worker));
        app.message = "Waiting for the publisher's uninstaller".into();
        app.busy = true;
        events
            .send(Event::Error(
                "Registered uninstaller no longer exists".into(),
            ))
            .unwrap();
        app.receive_background(&ctx);
        assert!(!app.busy);
        assert!(!app.message.contains("Waiting"));
        assert!(app.error.is_some());
    }
    #[test]
    fn uac_cancellation_is_not_removal_and_successful_launch_clears_the_offer() {
        let ctx = egui::Context::default();
        let mut ui = Desktop::preview(&ctx, "applications-uac-cancelled").unwrap();
        assert!(ui.message.contains("canceled"));
        assert!(!ui.message.contains("uninstaller is running"));
        assert!(ui.error.is_none() && ui.snapshot.is_none() && ui.removed_rows.is_empty());
        assert!(ui.removal_view.as_ref().unwrap().elevation.is_some());
        ui.apply_removal(
            crate::worker::RemovalUpdate {
                app: ui.apps[0].clone(),
                status: Status {
                    phase: Phase::Running,
                    process_running: true,
                    exit_code: None,
                    monitoring: true,
                },
                problem: None,
                snapshot: None,
                elevation: None,
            },
            Instant::now(),
        );
        assert!(ui.removal_view.as_ref().unwrap().elevation.is_none());
        assert!(ui.message.contains("uninstaller is running"));
    }
    #[test]
    fn removed_row_expires_once_and_later_monitor_completion_does_not_resurrect_it() {
        let ctx = egui::Context::default();
        let mut ui = Desktop::preview(&ctx, "applications-removed").unwrap();
        let app = ui.apps[0].clone();
        let marked = ui.removed_rows[&app.id];
        let now = marked + removal::ROW_GRACE + removal::ROW_COLLAPSE;
        ui.preview_now = Some(now);
        ui.update_removed_rows(&ctx);
        assert!(!ui.apps.iter().any(|entry| entry.id == app.id));
        assert!(ui.selected.is_none());
        ui.apply_removal(
            crate::worker::RemovalUpdate {
                app: app.clone(),
                status: Status {
                    phase: Phase::Removed,
                    process_running: false,
                    exit_code: Some(0),
                    monitoring: false,
                },
                problem: None,
                snapshot: None,
                elevation: None,
            },
            now,
        );
        assert!(!ui.apps.iter().any(|entry| entry.id == app.id));
        ui.apply_removal(
            crate::worker::RemovalUpdate {
                app: app.clone(),
                status: Status {
                    phase: Phase::Running,
                    process_running: true,
                    exit_code: None,
                    monitoring: true,
                },
                problem: None,
                snapshot: None,
                elevation: None,
            },
            now,
        );
        assert!(ui.apps.iter().any(|entry| entry.id == app.id));
        assert!(!ui.removed_rows.contains_key(&app.id));
    }
    #[test]
    fn refresh_keeps_the_grace_row_but_a_reappearing_registration_cancels_it() {
        let ctx = egui::Context::default();
        let mut ui = Desktop::preview(&ctx, "applications-removed").unwrap();
        let app = ui.apps[0].clone();
        let marked = ui.removed_rows[&app.id];
        ui.set_inventory(Inventory::default(), &ctx);
        assert!(ui.apps.iter().any(|entry| entry.id == app.id));
        assert_eq!(ui.removed_rows[&app.id], marked);
        ui.set_inventory(
            Inventory {
                apps: vec![app.clone()],
                warnings: vec![],
            },
            &ctx,
        );
        assert!(!ui.removed_rows.contains_key(&app.id));
        assert_ne!(
            ui.removal_view.as_ref().unwrap().status.phase,
            Phase::Removed
        );
    }
}
