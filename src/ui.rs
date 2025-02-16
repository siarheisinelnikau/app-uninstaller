use crate::{
    REPOSITORY, VERSION,
    command::UninstallPlan,
    inventory::{InstalledApp, Inventory},
    leftovers::{Candidate, QuarantineRecord, Snapshot},
    settings::Settings,
    updater::Update,
    worker::{Event, Work, Worker},
};
use egui::{self, Color32, RichText};
use std::{collections::HashMap, path::PathBuf};

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
    message: String,
    error: Option<String>,
    confirmation: Option<Confirmation>,
    snapshot: Option<Snapshot>,
    candidate: Option<Candidate>,
    candidate_selected: bool,
    restore_confirm: Option<QuarantineRecord>,
    records: Vec<QuarantineRecord>,
    settings: Settings,
    data: Option<PathBuf>,
    update: Option<Update>,
    update_status: String,
    update_busy: bool,
    progress: Option<(u64, u64)>,
    staged: Option<PathBuf>,
    saved_settings: Option<Settings>,
}
impl Desktop {
    /// Isolated real UI with deterministic data and no worker or external effects.
    pub fn preview(ctx: &egui::Context, screen: &str) -> anyhow::Result<Self> {
        let mut app = Self::create(ctx, None);
        app.page = match screen {
            "applications" | "applications-load" => Page::Applications,
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
        if screen == "applications-load" {
            for index in 6..840 {
                let mut entry = app.apps[index % 6].clone();
                entry.id = format!("preview/{index}");
                entry.name = format!("{} {index:03}", entry.name);
                entry.system_component = index % 4 != 0;
                app.apps.push(entry);
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
            data: None,
            update: None,
            update_status: "No check completed in this session.".into(),
            update_busy: false,
            progress: None,
            staged: None,
            saved_settings: None,
        }
    }
    fn send(&mut self, work: Work, message: &str) {
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
        self.apps = inventory.apps;
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
    fn receive(&mut self, ctx: &egui::Context) {
        while let Some(event) = self
            .worker
            .as_ref()
            .and_then(|worker| worker.rx.try_recv().ok())
        {
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
                Event::Uninstalled {
                    app_name,
                    code,
                    snapshot,
                    inventory,
                } => {
                    self.set_inventory(inventory, ctx);
                    self.busy = false;
                    self.candidate = None;
                    self.snapshot = if [3010, 1641].contains(&code) {
                        None
                    } else {
                        snapshot
                    };
                    self.page = Page::Leftovers;
                    self.message = if [3010, 1641].contains(&code) {
                        format!(
                            "{app_name}: uninstaller reports a Windows restart. Leftover quarantine is disabled for this session; finish the restart first."
                        )
                    } else {
                        format!(
                            "{app_name}: uninstaller process exited with code {code}. Finish any remaining publisher windows before reviewing. Registration removal is checked separately."
                        )
                    };
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
                    self.message = format!(
                        "{} moved into reversible quarantine. Disk space has not been reclaimed.",
                        record.app_name
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
                            self.update_status =
                                format!("Version {} is available.", update.version);
                            self.update = Some(update);
                        }
                        Ok(None) => {
                            self.update_status =
                                "You are up to date for the stable channel.".into();
                            self.update = None;
                        }
                        Err(e) => {
                            self.update_status = format!("Could not check for updates: {e}");
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
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Event::Error(error) => {
                    self.busy = false;
                    self.progress = None;
                    self.error = Some(error);
                    if let Some(settings) = self.saved_settings.take() {
                        self.settings = settings;
                    }
                }
            }
        }
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
        use crate::design::*;
        ui.heading("Installed applications");
        ui.label(
            RichText::new(
                "Choose an app and review its registered uninstaller before making changes.",
            )
            .color(MUTED),
        );
        ui.add_space(10.);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search by name or publisher")
                        .desired_width(300.),
                )
                .changed()
            {
                self.filter_dirty = true;
            }
            egui::ComboBox::from_id_salt("sort")
                .selected_text(match self.sort {
                    Sort::Name => "Name",
                    Sort::Publisher => "Publisher",
                    Sort::Size => "Largest first",
                })
                .show_ui(ui, |ui| {
                    for (sort, label) in [
                        (Sort::Name, "Name"),
                        (Sort::Publisher, "Publisher"),
                        (Sort::Size, "Largest first"),
                    ] {
                        if ui.selectable_value(&mut self.sort, sort, label).changed() {
                            self.filter_dirty = true;
                        }
                    }
                });
            if ui
                .add_enabled(!self.busy, egui::Button::new("Refresh"))
                .clicked()
            {
                self.send(Work::Refresh, "Refreshing installed applications…");
            }
        });
        ui.horizontal(|ui| {
            if ui
                .checkbox(
                    &mut self.show_system,
                    "Include system components (view only)",
                )
                .changed()
            {
                self.filter_dirty = true;
            }
            self.rebuild_filter();
            ui.label(
                RichText::new(format!("{} apps shown", self.visible.len()))
                    .small()
                    .color(MUTED),
            );
        });
        ui.add_space(6.);
        if !self.inventory_warnings.is_empty() {
            ui.colored_label(Color32::from_rgb(150,70,0),"Inventory is incomplete. Review the read errors in Settings before removing an app.");
        }
        ui.columns(2,|columns| {
            egui::ScrollArea::vertical().auto_shrink([false,false]).id_salt("inventory").show_rows(&mut columns[0],70.,self.visible.len(),|ui,range| {
                for row in range {
                    let app=&self.apps[self.visible[row]];
                    let selected=self.selected.as_ref()==Some(&app.id);
                    let width=ui.available_width();
                    let response=egui::Frame::new().fill(if selected {SELECTION} else {Color32::TRANSPARENT}).inner_margin(10.).show(ui,|ui| {
                        ui.set_width((width-20.).max(1.));
                        ui.horizontal(|ui| {
                            if let Some(icon)=self.icons.get(&app.id) {ui.image((icon.id(),egui::vec2(30.,30.)));}
                            ui.vertical(|ui| {
                                ui.spacing_mut().button_padding=egui::vec2(0.,2.);
                                if ui.add(egui::Button::new(RichText::new(&app.name).strong()).frame(false).truncate()).clicked() {self.selected=Some(app.id.clone());}
                                ui.add(egui::Label::new(RichText::new(format!("{} · {}",if app.publisher.is_empty(){"Unknown publisher"}else{&app.publisher},size(app.estimated_kib.map(|n|n*1024)))).small().color(MUTED)).truncate());
                            });
                        });
                    });
                    if response.response.interact(egui::Sense::click()).clicked() {self.selected=Some(app.id.clone());}
                }
            });
            egui::ScrollArea::vertical().auto_shrink([false,false]).id_salt("app-details").show(&mut columns[1],|ui| {
                egui::Frame::new().fill(SURFACE).inner_margin(20.).show(ui,|ui| {
                    ui.set_width((ui.available_width()).max(1.));
                    let app=self.apps.iter().find(|a|self.selected.as_ref()==Some(&a.id)).cloned();
                    if let Some(app)=app {
                        ui.heading(&app.name);
                        ui.label(RichText::new(if app.publisher.is_empty(){"Publisher not supplied"}else{&app.publisher}).color(MUTED));
                        ui.add_space(8.);
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!self.busy&&!app.system_component&&self.inventory_warnings.is_empty(),egui::Button::new(RichText::new("Review uninstall…").color(Color32::WHITE)).fill(ACCENT)).clicked() {self.send(Work::Prepare(app.clone()),"Validating registration and capturing bounded ownership evidence…");}
                        });
                        ui.label(RichText::new("Nothing changes until you confirm the registered action.").small().color(MUTED));
                        ui.add_space(12.);
                        let value_width=(ui.available_width()-116.).max(90.);
                        egui::Grid::new("metadata").num_columns(2).min_col_width(96.).max_col_width(value_width).spacing([16.,8.]).show(ui,|ui| {
                            for (label,value) in [("Version",app.version.clone()),("Size",size(app.estimated_kib.map(|n|n*1024))),("Installed for",format!("{} · {}",app.scope,app.view)),("Folder",app.install_location.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||"Not supplied".into()))] {
                                ui.add(egui::Label::new(RichText::new(label).color(MUTED)).wrap_mode(egui::TextWrapMode::Extend));
                                ui.add(egui::Label::new(if value.is_empty(){"Not supplied"}else{&value}).halign(egui::Align::Min).wrap());ui.end_row();
                            }
                        });
                        ui.add_space(16.);ui.strong("Registered uninstall command");
                        egui::Frame::new().fill(BACKGROUND).inner_margin(10.).show(ui,|ui| {
                            ui.add(egui::Label::new(RichText::new(if app.uninstall_string.is_empty(){"Not supplied"}else{&app.uninstall_string}).monospace()).halign(egui::Align::Min).selectable(true).wrap());
                        });
                        ui.add_space(10.);
                        ui.collapsing("How removal and recovery work",|ui| {
                            ui.label("After uninstalling, review proven leftovers and choose whether to quarantine them. Quarantine is reversible; the publisher’s uninstall is not.");
                            ui.small("Uncertain ownership and unsupported command formats are refused. Administrator-only uninstallers may need Windows Installed apps. No automatic elevation or registry cleaning.");
                        });
                    } else {
                        ui.heading("Select an application");
                        ui.label("See its publisher, installation folder and registered removal action here.");
                        ui.add_space(12.);
                        ui.label("Review → Uninstall → Inspect leftovers → Quarantine if needed");
                    }
                });
            });
        });
    }
    fn leftovers(&mut self, ui: &mut egui::Ui) {
        let mut action = None;
        ui.heading("Explainable leftover review");
        ui.label("Only a proven installation folder can be suggested. Documents, shared locations, custom paths and registry values are excluded. Missing ownership evidence means no cleanup suggestion.");
        if let Some(snapshot) = &self.snapshot {
            ui.add_space(10.);
            ui.strong(&snapshot.app_name);
            ui.monospace(snapshot.root.display().to_string());
            if ui
                .add_enabled(
                    !self.busy,
                    egui::Button::new("Publisher uninstaller finished · Review leftovers"),
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
            ui.label("No removal session has evidence to review. Start with Review uninstall in Applications. Evidence is kept in memory for this session only.");
        }
        if let Some(candidate) = &self.candidate {
            ui.separator();
            ui.heading("Folder preview");
            ui.monospace(candidate.snapshot.root.display().to_string());
            let bytes = candidate.files.values().map(|f| f.bytes).sum();
            ui.label(format!(
                "{} unchanged files and folders · {}",
                candidate.files.len(),
                size(Some(bytes))
            ));
            for evidence in &candidate.snapshot.evidence {
                ui.label(format!("• {evidence}"));
            }
            ui.collapsing("Every proposed file/folder and file SHA-256", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(200.)
                    .show(ui, |ui| {
                        for (path, fingerprint) in &candidate.files {
                            ui.monospace(path.display().to_string());
                            if fingerprint.directory {
                                ui.small("Folder");
                            } else {
                                ui.small(format!(
                                    "{} · {}",
                                    size(Some(fingerprint.bytes)),
                                    fingerprint.sha256
                                ));
                            }
                        }
                        if candidate.files.is_empty() {
                            ui.label("Only the empty folder remains.");
                        }
                    });
            });
            ui.add_space(10.);
            ui.checkbox(
                &mut self.candidate_selected,
                "I reviewed this folder and select it for reversible quarantine",
            );
            ui.label("Quarantine renames the folder beside its original location. It preserves data and uses the same disk space. Restore is available in Quarantine. Access errors leave the original in place.");
            if ui
                .add_enabled(
                    !self.busy && self.candidate_selected && self.data.is_some(),
                    egui::Button::new("Quarantine selected folder"),
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
        ui.heading("Recoverable quarantine");
        ui.label("Quarantined folders remain on the same drive. Restore never overwrites an existing location. Restoring files does not reinstall a program. Permanent deletion is intentionally manual.");
        if self.records.is_empty() {
            ui.add_space(15.);
            ui.label("No quarantined folders.");
        }
        for record in &self.records {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong(&record.app_name);
                ui.label(format!("Original: {}", record.original.display()));
                ui.label(format!("Quarantine: {}", record.backup.display()));
                ui.small(format!(
                    "{} files and folders · journal {}",
                    record.files.len(),
                    record.id
                ));
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Review restore…"))
                    .clicked()
                {
                    self.restore_confirm = Some(record.clone());
                }
            });
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.set_max_width(760.);
        ui.heading("Settings & updates");
        ui.label(format!(
            "AppUninstaller {VERSION} · Windows x64 portable · MIT license"
        ));
        ui.add_space(10.);
        ui.strong("Updates");
        ui.hyperlink_to("Canonical repository", REPOSITORY);
        ui.add_space(10.);
        let previous = self.settings.clone();
        if ui
            .add_enabled(
                !self.busy && self.data.is_some(),
                egui::Checkbox::new(
                    &mut self.settings.check_on_startup,
                    "Check for updates on startup",
                ),
            )
            .changed()
        {
            self.saved_settings = Some(previous);
            self.send(Work::Save(self.settings.clone()), "Saving settings…");
        }
        ui.small("Checks quietly in the background. Turning this off prevents startup requests; manual checking stays available. Downloads and installation require your action.");
        ui.label(&self.update_status);
        if ui
            .add_enabled(
                !self.busy && !self.update_busy,
                egui::Button::new("Check for updates"),
            )
            .clicked()
        {
            self.update_busy = true;
            self.send(Work::CheckUpdate, "Checking for updates…");
        }
        if let Some(update) = self.update.clone() {
            ui.separator();
            ui.strong(format!("Update {}", update.version));
            ui.collapsing("Release notes", |ui| {
                ui.label(&update.notes);
            });
            ui.label("Download is verified against the release SHA-256 before installation. The existing EXE is retained as a rollback backup. Settings and quarantine records stay in your user profile.");
            if self.staged.is_none()
                && ui
                    .add_enabled(!self.busy, egui::Button::new("Download and install"))
                    .clicked()
            {
                self.send(Work::Download(update), "Downloading and verifying update…");
            }
        }
        if let Some((done, total)) = self.progress {
            ui.add(
                egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(format!(
                    "{} / {}",
                    size(Some(done)),
                    size(Some(total))
                )),
            );
        }
        if let Some(path) = self.staged.clone() {
            ui.label(
                "Verified download ready. Restart installs the update after this process exits.",
            );
            if ui
                .add_enabled(
                    !self.busy,
                    egui::Button::new("Restart and install verified update"),
                )
                .clicked()
            {
                self.send(Work::Install(path), "Starting replacement helper…");
            }
        }
        ui.separator();
        ui.strong("About & local storage");
        ui.label("CPU rendering · Windows GDI · no graphics API context");
        if let Some(data) = &self.data {
            ui.monospace(data.display().to_string());
        } else {
            ui.label("User data storage unavailable. Settings and quarantine are disabled.");
        }
        ui.small("No telemetry. No registry cleaning. Inventory stays local. Network access is used only for updates.");
        ui.collapsing("Open-source licenses and bundled font notices", |ui| {
            static LINES: std::sync::LazyLock<Vec<&'static str>> = std::sync::LazyLock::new(|| {
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
        });
        for warning in self.warnings.iter().chain(self.inventory_warnings.iter()) {
            ui.colored_label(Color32::from_rgb(150, 70, 0), warning);
        }
    }
    fn confirmations(&mut self, ctx: &egui::Context) {
        let mut launch = false;
        let mut cancel = false;
        if let Some(confirm) = &mut self.confirmation {
            egui::Window::new("Review registered uninstall action").collapsible(false).resizable(true).default_width(640.).show(ctx,|ui| {
                ui.heading(&confirm.app.name);ui.label(format!("Publisher: {}",confirm.app.publisher));
                ui.strong("Executable");ui.monospace(confirm.plan.executable.display().to_string());
                ui.strong("Arguments");ui.monospace(&confirm.plan.arguments);ui.label(&confirm.plan.explanation);
                if let Some(limitation)=&confirm.limitation {ui.colored_label(Color32::from_rgb(150,70,0),format!("Leftover review unavailable: {limitation}"));}
                else {ui.label("Ownership evidence captured. A leftover suggestion still requires successful registration removal and unchanged file hashes.");}
                ui.label("This runs third-party software which may change files and settings. Save your work first. AppUninstaller cannot undo the publisher's uninstall.");
                ui.checkbox(&mut confirm.accepted,"I reviewed this action and want to run the registered uninstaller");
                ui.horizontal(|ui|{cancel=ui.button("Cancel").clicked();launch=ui.add_enabled(confirm.accepted&&!self.busy,egui::Button::new("Run registered uninstaller")).clicked();});
            });
        }
        if cancel {
            self.confirmation = None;
        }
        if launch && let Some(confirm) = self.confirmation.take() {
            self.snapshot = confirm.snapshot.clone();
            self.candidate = None;
            self.send(
                Work::Uninstall(Box::new(confirm.app), confirm.plan, confirm.snapshot),
                "Waiting for the publisher's uninstaller. Complete its windows to continue…",
            );
        }
        let mut restore = false;
        let mut dismiss = false;
        if let Some(record) = &self.restore_confirm {
            egui::Window::new("Restore quarantined folder").collapsible(false).show(ctx,|ui| {
                ui.label(format!("Restore {} to:",record.app_name));ui.monospace(record.original.display().to_string());
                ui.label("Files must still match the quarantine journal. An existing original location will never be overwritten.");
                ui.horizontal(|ui|{dismiss=ui.button("Cancel").clicked();restore=ui.add_enabled(!self.busy,egui::Button::new("Restore folder")).clicked();});
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
        self.receive(ctx);
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(crate::design::SURFACE)
                    .inner_margin(egui::Margin::symmetric(24, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("AppUninstaller")
                            .size(24.)
                            .strong()
                            .color(crate::design::ACCENT),
                    );
                    ui.label(
                        RichText::new("A careful way to remove apps.").color(crate::design::MUTED),
                    );
                });

                ui.horizontal(|ui| {
                    for (page, label) in [
                        (Page::Applications, "Applications"),
                        (Page::Leftovers, "Leftover review"),
                        (Page::Quarantine, "Quarantine"),
                        (Page::Settings, "Settings"),
                    ] {
                        ui.selectable_value(&mut self.page, page, label);
                    }
                    if self.update.is_some() && ui.button("Update available").clicked() {
                        self.page = Page::Settings;
                    }
                });
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            if let Some(error) = self.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(Color32::from_rgb(160, 40, 30), error);
                    if ui.small_button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                if self.busy {
                    ui.spinner();
                }
                ui.label(&self.message);
            });
        });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(crate::design::BACKGROUND)
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
        self.confirmations(ctx);
    }
}
fn size(bytes: Option<u64>) -> String {
    match bytes {
        None => "Size not supplied".into(),
        Some(n) if n >= 1024 * 1024 * 1024 => {
            format!("{:.1} GB", n as f64 / (1024. * 1024. * 1024.))
        }
        Some(n) if n >= 1024 * 1024 => format!("{:.1} MB", n as f64 / (1024. * 1024.)),
        Some(n) => format!("{:.1} KB", n as f64 / 1024.),
    }
}
