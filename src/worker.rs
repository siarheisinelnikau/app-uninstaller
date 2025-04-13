//! One bounded, sleeping worker owns filesystem, registry, process and network work.
use crate::{
    command::{self, UninstallPlan},
    inventory::{self, InstalledApp, Inventory},
    leftovers::{self, Candidate, QuarantineRecord, SafetyPolicy, Snapshot},
    removal::{self, Observer, Phase, Presence, Status},
    settings::{self, Settings},
    updater::{self, Update},
};
use anyhow::{Context, Result, ensure};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender},
    time::Instant,
};

pub enum Work {
    Refresh,
    Prepare(InstalledApp),
    Uninstall(Box<InstalledApp>, UninstallPlan, Option<Snapshot>),
    RetryAsAdministrator(String),
    Review(Snapshot),
    Quarantine(Candidate),
    Restore(QuarantineRecord),
    Save(Settings),
    CheckUpdate,
    Download(Update),
    Install(updater::StagedUpdate),
    StopWatching,
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
    Removal(Box<RemovalUpdate>),
    Reviewed(Option<Candidate>, Inventory),
    Quarantined(QuarantineRecord),
    Restored(String),
    Saved,
    UpdateStarted,
    UpdateChecked(std::result::Result<Option<Update>, String>),
    Progress(u64, u64),
    Downloaded(updater::StagedUpdate),
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
    watch: Option<WatchedRemoval>,
}
pub struct RemovalUpdate {
    pub app: InstalledApp,
    pub status: Status,
    pub problem: Option<String>,
    pub snapshot: Option<Snapshot>,
    pub elevation: Option<ElevationOffer>,
}
#[derive(Clone)]
pub struct ElevationOffer {
    pub plan: UninstallPlan,
    pub cancelled: bool,
}
struct PendingElevation {
    offer: ElevationOffer,
    snapshot: Option<Snapshot>,
}
struct WatchedRemoval {
    app: InstalledApp,
    child: Option<command::RunningUninstaller>,
    exit_code: Option<u32>,
    snapshot: Option<Snapshot>,
    launch_error: Option<String>,
    elevation: Option<PendingElevation>,
    observer: Observer,
    started: Instant,
    next_check: Instant,
    last: Option<(Status, Option<String>)>,
}
impl WatchedRemoval {
    fn poll(&mut self) -> Option<RemovalUpdate> {
        let now = Instant::now();
        if now < self.next_check {
            return None;
        }
        self.next_check = now + removal::CHECK_INTERVAL;
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(code)) => {
                    self.exit_code = Some(code);
                    self.child = None;
                }
                Ok(None) => {}
                Err(error) => {
                    self.launch_error = Some(format!(
                        "Could not read the publisher process state: {error}"
                    ));
                    self.child = None;
                    self.snapshot = None;
                }
            }
        }
        let (presence, problem) = match inventory::registration_exists(&self.app) {
            Ok(true) => (Presence::Present, self.launch_error.clone()),
            Ok(false) => (Presence::Missing, self.launch_error.clone()),
            Err(error) => (
                Presence::Unknown,
                Some(format!(
                    "Cannot verify installed-app registration: {error:#}"
                )),
            ),
        };
        let status = self.observer.observe(
            now.duration_since(self.started),
            presence,
            self.child.is_some(),
            self.exit_code,
        );
        if self.last.as_ref() == Some(&(status.clone(), problem.clone())) {
            return None;
        }
        self.last = Some((status.clone(), problem.clone()));
        Some(self.update(status, problem))
    }
    fn update(&self, status: Status, problem: Option<String>) -> RemovalUpdate {
        let snapshot = if status.phase == Phase::Removed
            && !status.process_running
            && status
                .exit_code
                .is_some_and(|code| ![3010, 1641].contains(&code))
        {
            self.snapshot.clone()
        } else {
            None
        };
        RemovalUpdate {
            app: self.app.clone(),
            elevation: if status.monitoring
                && status.phase != Phase::Removed
                && self.child.is_none()
            {
                self.elevation.as_ref().map(|pending| pending.offer.clone())
            } else {
                None
            },
            status,
            problem,
            snapshot,
        }
    }
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
                watch: None,
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

    fn perform(&mut self, work: Work, publish: &impl Fn(Event) -> bool) -> Result<Event> {
        ensure!(
            self.watch.is_none()
                || !matches!(
                    &work,
                    Work::Prepare(_)
                        | Work::Uninstall(..)
                        | Work::CheckUpdate
                        | Work::Download(_)
                        | Work::Install(_)
                ),
            "Finish or stop watching the current removal before starting another removal or update operation"
        );
        Ok(match work {
            Work::Refresh => Event::Inventory(inventory::enumerate()?),
            Work::Prepare(app) => {
                let inventory = self.complete_inventory()?;
                let Some(current) = inventory.apps.iter().find(|entry| entry.id == app.id) else {
                    return Ok(self.start_watch(app, None, None, None, None));
                };
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
                let Some(current) = inventory.apps.iter().find(|entry| entry.id == app.id) else {
                    return Ok(self.start_watch(*app, None, None, None, None));
                };
                let plan = command::validate_approved(
                    current,
                    &app,
                    &approved,
                    &command::windows_directory()?,
                )?;
                match command::spawn(&plan) {
                    Ok(child) => self.start_watch(*app, Some(child), None, snapshot, None),
                    Err(error)
                        if command::launch_error_kind(&error)
                            == command::LaunchErrorKind::ElevationRequired =>
                    {
                        self.start_watch(
                            *app,
                            None,
                            None,
                            None,
                            Some(PendingElevation {
                                offer: ElevationOffer {
                                    plan,
                                    cancelled: false,
                                },
                                snapshot,
                            }),
                        )
                    }
                    Err(error) => self.start_watch(
                        *app,
                        None,
                        Some(format!("Nothing was launched: {error:#}")),
                        None,
                        None,
                    ),
                }
            }
            Work::RetryAsAdministrator(id) => self.retry_elevated(&id)?,
            Work::StopWatching => {
                let watch = self.watch.take().context("No removal is being observed")?;
                let mut status = watch
                    .last
                    .as_ref()
                    .map(|(status, _)| status.clone())
                    .context("Removal has no observation")?;
                status.monitoring = false;
                if status.phase != Phase::Removed {
                    status.phase = Phase::Unconfirmed;
                }
                Event::Removal(Box::new(watch.update(status, None)))
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
    fn start_watch(
        &mut self,
        app: InstalledApp,
        child: Option<command::RunningUninstaller>,
        problem: Option<String>,
        snapshot: Option<Snapshot>,
        elevation: Option<PendingElevation>,
    ) -> Event {
        let now = Instant::now();
        let mut watch = WatchedRemoval {
            app,
            child,
            exit_code: None,
            snapshot,
            launch_error: problem,
            elevation,
            observer: Observer::default(),
            started: now,
            next_check: now,
            last: None,
        };
        let update = watch
            .poll()
            .expect("First removal observation always publishes a status");
        self.watch = Some(watch);
        Event::Removal(Box::new(update))
    }
    fn retry_elevated(&mut self, id: &str) -> Result<Event> {
        let watch = self.watch.as_ref().context(
            "Review the uninstall command again before requesting administrator permission",
        )?;
        ensure!(
            watch.app.id == id && watch.child.is_none() && watch.elevation.is_some(),
            "This removal has no pending administrator retry"
        );
        ensure!(
            !watch
                .last
                .as_ref()
                .is_some_and(|(status, _)| status.phase == Phase::Removed),
            "The app is already removed"
        );
        let mut watch = self.watch.take().expect("Pending watch was checked");
        let mut pending = watch
            .elevation
            .take()
            .expect("Pending elevation was checked");
        // The approved command is worker-owned. Never accept an arbitrary executable
        // from the retry button, and never launch after a changed registration.
        let launch = (|| -> Result<Option<command::RunningUninstaller>> {
            let inventory = self.complete_inventory()?;
            let Some(current) = inventory.apps.iter().find(|entry| entry.id == id) else {
                return Ok(None);
            };
            let fresh = command::validate_approved(
                current,
                &watch.app,
                &pending.offer.plan,
                &command::windows_directory()?,
            )?;
            command::spawn_elevated(&fresh)
        })();
        Ok(match launch {
            Ok(child) => {
                let snapshot = if child.is_some() {
                    pending.snapshot
                } else {
                    None
                };
                self.start_watch(watch.app, child, None, snapshot, None)
            }
            Err(error)
                if command::launch_error_kind(&error) == command::LaunchErrorKind::Cancelled =>
            {
                pending.offer.cancelled = true;
                self.start_watch(watch.app, None, None, None, Some(pending))
            }
            Err(error) => self.start_watch(
                watch.app,
                None,
                Some(format!("Administrator launch did not complete: {error:#}")),
                None,
                None,
            ),
        })
    }
    fn poll_watch(&mut self) -> Option<Event> {
        let update = self.watch.as_mut()?.poll()?;
        if !update.status.monitoring {
            self.watch = None;
        }
        Some(Event::Removal(Box::new(update)))
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
            let (mut state, ready, startup) = State::load();
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
            loop {
                let work = if state.watch.is_some() {
                    match commands.recv_timeout(removal::CHECK_INTERVAL) {
                        Ok(work) => Some(work),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                } else {
                    match commands.recv() {
                        Ok(work) => Some(work),
                        Err(_) => break,
                    }
                };
                if let Some(work) = work {
                    let event = state
                        .perform(work, &publish)
                        .unwrap_or_else(|error| Event::Error(format!("{error:#}")));
                    if !publish(event) {
                        break;
                    }
                }
                if let Some(event) = state.poll_watch()
                    && !publish(event)
                {
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

#[cfg(test)]
mod tests {
    use super::*;
    fn pending_watch() -> WatchedRemoval {
        let now = Instant::now();
        WatchedRemoval {
            app: InstalledApp {
                id: "fixture".into(),
                ..Default::default()
            },
            child: None,
            exit_code: None,
            snapshot: None,
            launch_error: None,
            elevation: Some(PendingElevation {
                offer: ElevationOffer {
                    plan: command::parse_registered(r#""C:\Fixture\remove.exe" /uninstall"#)
                        .unwrap(),
                    cancelled: true,
                },
                snapshot: None,
            }),
            observer: Observer::default(),
            started: now,
            next_check: now,
            last: None,
        }
    }
    #[test]
    fn canceled_uac_keeps_retry_but_removal_and_stop_withdraw_it() {
        let watch = pending_watch();
        let mut status = Status {
            phase: Phase::Waiting,
            process_running: false,
            exit_code: None,
            monitoring: true,
        };
        let update = watch.update(status.clone(), None);
        assert!(update.elevation.unwrap().cancelled);
        assert!(update.snapshot.is_none());
        status.phase = Phase::Removed;
        assert!(watch.update(status.clone(), None).elevation.is_none());
        status.phase = Phase::Unconfirmed;
        status.monitoring = false;
        assert!(watch.update(status, None).elevation.is_none());
    }
    #[test]
    fn stale_or_unoffered_administrator_retry_is_rejected_before_any_launch() {
        let mut state = State {
            data: None,
            policy: SafetyPolicy::windows(),
            watch: Some(pending_watch()),
        };
        assert!(state.retry_elevated("different-app").is_err());
        state.watch.as_mut().unwrap().elevation = None;
        assert!(state.retry_elevated("fixture").is_err());
        assert!(state.watch.is_some());
    }
}
