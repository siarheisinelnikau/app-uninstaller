//! One bounded, sleeping worker owns filesystem, registry, process and network work.
use crate::{
    command::{self, UninstallPlan},
    inventory::{self, InstalledApp, Inventory},
    leftovers::{self, Candidate, QuarantineRecord, SafetyPolicy, Snapshot},
    settings::{self, Settings},
    updater::{self, Update},
};
use anyhow::{Context, Result, ensure};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender},
};

pub enum Work {
    Refresh,
    Prepare(InstalledApp),
    Uninstall(Box<InstalledApp>, UninstallPlan, Option<Snapshot>),
    Review(Snapshot),
    Quarantine(Candidate),
    Restore(QuarantineRecord),
    Save(Settings),
    CheckUpdate,
    Download(Update),
    Install(PathBuf),
}
pub enum Event {
    Ready {
        settings: Settings,
        data: Option<PathBuf>,
        inventory: Inventory,
        records: Vec<QuarantineRecord>,
        warnings: Vec<String>,
    },
    Inventory(Inventory),
    Prepared {
        app: Box<InstalledApp>,
        plan: UninstallPlan,
        snapshot: Option<Snapshot>,
        limitation: Option<String>,
    },
    Uninstalled {
        app_name: String,
        code: u32,
        snapshot: Option<Snapshot>,
        inventory: Inventory,
    },
    Reviewed(Option<Candidate>, Inventory),
    Quarantined(QuarantineRecord),
    Restored(String),
    Saved,
    UpdateStarted,
    UpdateChecked(std::result::Result<Option<Update>, String>),
    Progress(u64, u64),
    Downloaded(PathBuf),
    Installing,
    Error(String),
}
pub struct Worker {
    tx: SyncSender<Work>,
    pub rx: Receiver<Event>,
}
struct State {
    data: Option<PathBuf>,
    policy: SafetyPolicy,
}

impl State {
    fn load() -> (Self, Event, bool) {
        let mut warnings = Vec::new();
        let data = match settings::data_dir() {
            Ok(path) => Some(path),
            Err(error) => {
                warnings.push(error.to_string());
                None
            }
        };
        let settings = match data.as_ref().map(|dir| settings::load(dir)) {
            Some(Ok(settings)) => settings,
            Some(Err(error)) => {
                warnings.push(format!("Settings unavailable: {error}. Startup update check is disabled for this session"));
                Settings {
                    check_on_startup: false,
                }
            }
            None => Settings {
                check_on_startup: false,
            },
        };
        let records = match data
            .as_ref()
            .map(|dir| leftovers::records(&dir.join("quarantine")))
        {
            Some(Ok(records)) => records,
            Some(Err(error)) => {
                warnings.push(format!("Quarantine journal: {error}"));
                vec![]
            }
            None => vec![],
        };
        if let Ok(Some(result)) = updater::previous_result() {
            warnings.push(result);
        }
        let inventory = inventory::enumerate().unwrap_or_else(|error| Inventory {
            apps: vec![],
            warnings: vec![error.to_string()],
        });
        let startup = settings.check_on_startup;
        let ready = Event::Ready {
            settings,
            data: data.clone(),
            inventory,
            records,
            warnings,
        };
        (
            Self {
                data,
                policy: SafetyPolicy::windows(),
            },
            ready,
            startup,
        )
    }

    fn complete_inventory(&self) -> Result<Inventory> {
        let inventory = inventory::enumerate()?;
        ensure!(
            inventory.warnings.is_empty(),
            "Inventory is incomplete. Resolve registry read errors and refresh before performing this action"
        );
        Ok(inventory)
    }

    fn perform(&self, work: Work, publish: &impl Fn(Event) -> bool) -> Result<Event> {
        Ok(match work {
            Work::Refresh => Event::Inventory(inventory::enumerate()?),
            Work::Prepare(app) => {
                let inventory = self.complete_inventory()?;
                let current = inventory
                    .apps
                    .iter()
                    .find(|entry| entry.id == app.id)
                    .context("Registration no longer exists. Refresh the list")?;
                ensure!(
                    !current.system_component,
                    "System components are inspection only"
                );
                let plan = command::plan(current, &command::windows_directory()?)?;
                let (snapshot, limitation) =
                    match leftovers::capture(current, &inventory.apps, &self.policy) {
                        Ok(snapshot) => (Some(snapshot), None),
                        Err(error) => (None, Some(format!("{error:#}"))),
                    };
                Event::Prepared {
                    app: Box::new(current.clone()),
                    plan,
                    snapshot,
                    limitation,
                }
            }
            Work::Uninstall(app, approved, snapshot) => {
                let inventory = self.complete_inventory()?;
                let current = inventory
                    .apps
                    .iter()
                    .find(|entry| entry.id == app.id)
                    .context("Registration disappeared before launch")?;
                let plan = command::plan(current, &command::windows_directory()?)?;
                ensure!(
                    plan == approved
                        && current.install_location == app.install_location
                        && current.name == app.name
                        && current.publisher == app.publisher
                        && !current.system_component,
                    "Registration changed. Prepare and review it again"
                );
                let code = command::execute(&plan)?;
                Event::Uninstalled {
                    app_name: app.name,
                    code,
                    snapshot,
                    inventory: inventory::enumerate()?,
                }
            }
            Work::Review(snapshot) => {
                let inventory = self.complete_inventory()?;
                Event::Reviewed(
                    leftovers::review(&snapshot, &inventory.apps, &self.policy)?,
                    inventory,
                )
            }
            Work::Quarantine(candidate) => {
                let data = self
                    .data
                    .as_ref()
                    .context("Quarantine journal storage is unavailable")?;
                let inventory = self.complete_inventory()?;
                let fresh = leftovers::review(&candidate.snapshot, &inventory.apps, &self.policy)?
                    .context("Directory is already gone")?;
                ensure!(
                    fresh.files == candidate.files,
                    "Preview changed. Review leftovers again"
                );
                Event::Quarantined(leftovers::quarantine(&fresh, &data.join("quarantine"))?)
            }
            Work::Restore(record) => {
                leftovers::restore(&record)?;
                Event::Restored(record.id)
            }
            Work::Save(settings) => {
                settings::save(
                    self.data
                        .as_ref()
                        .context("Settings directory unavailable")?,
                    &settings,
                )?;
                Event::Saved
            }
            Work::CheckUpdate => {
                Event::UpdateChecked(updater::check().map_err(|error| format!("{error:#}")))
            }
            Work::Download(update) => {
                Event::Downloaded(updater::download(&update, |done, total| {
                    publish(Event::Progress(done, total));
                })?)
            }
            Work::Install(path) => {
                updater::launch_helper(&path)?;
                Event::Installing
            }
        })
    }
}

impl Worker {
    #[cfg(test)]
    pub(crate) fn test_channel() -> (Self, SyncSender<Event>) {
        let (tx, _commands) = mpsc::sync_channel(2);
        let (events, rx) = mpsc::sync_channel(16);
        (Self { tx, rx }, events)
    }
    pub fn start(ctx: egui::Context) -> Self {
        let (tx, commands) = mpsc::sync_channel::<Work>(2);
        let (events, rx) = mpsc::sync_channel::<Event>(16);
        let run = move || {
            let publish = |event| {
                let connected = events.send(event).is_ok();
                ctx.request_repaint();
                connected
            };
            let (state, ready, startup) = State::load();
            if !publish(ready) {
                return;
            }
            if startup {
                if !publish(Event::UpdateStarted) {
                    return;
                }
                if !publish(Event::UpdateChecked(
                    updater::check().map_err(|error| format!("{error:#}")),
                )) {
                    return;
                }
            }
            while let Ok(work) = commands.recv() {
                let event = state
                    .perform(work, &publish)
                    .unwrap_or_else(|error| Event::Error(format!("{error:#}")));
                if !publish(event) {
                    break;
                }
            }
        };
        std::thread::Builder::new()
            .name("application-worker".into())
            .spawn(run)
            .expect("Could not start application worker");
        Self { tx, rx }
    }
    pub fn send(&self, work: Work) -> std::result::Result<(), String> {
        self.tx
            .try_send(work)
            .map_err(|_| "The worker is busy. Wait for the current operation".into())
    }
}
