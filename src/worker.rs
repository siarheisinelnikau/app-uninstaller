use crate::{
    command::{self, UninstallPlan},
    inventory::{self, InstalledApp, Inventory},
    leftovers::{self, Candidate, QuarantineRecord, SafetyPolicy, Snapshot},
    settings::{self, Settings},
    updater::{self, Update},
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender},
};

pub enum Work {
    Refresh,
    Prepare(InstalledApp),
    Uninstall(InstalledApp, UninstallPlan, Option<Snapshot>),
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
        app: InstalledApp,
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
impl Worker {
    pub fn start(ctx: eframe::egui::Context) -> Self {
        let (tx, commands) = mpsc::sync_channel::<Work>(2);
        let (events, rx) = mpsc::sync_channel::<Event>(16);
        std::thread::Builder::new().name("application-worker".into()).spawn(move || {
            let publish = |event| { let ok = events.send(event).is_ok(); ctx.request_repaint(); ok };
            let mut warnings=Vec::new();
            let data = match settings::data_dir() { Ok(d)=>Some(d),Err(e)=>{warnings.push(e.to_string());None} };
            let settings = match data.as_ref().map(|d|settings::load(d)) {
                Some(Ok(s))=>s,Some(Err(e))=>{warnings.push(format!("Settings unavailable: {e}. Startup update check is disabled for this session"));Settings { check_on_startup:false }},None=>Settings { check_on_startup:false }
            };
            let records = data.as_ref().map(|d|leftovers::records(&d.join("quarantine"))).transpose().unwrap_or_else(|e|{warnings.push(format!("Quarantine journal: {e}"));None}).unwrap_or_default();
            let inventory = inventory::enumerate().unwrap_or_else(|e|Inventory { apps:vec![],warnings:vec![e.to_string()] });
            let startup=settings.check_on_startup;
            if !publish(Event::Ready {settings,data:data.clone(),inventory,records,warnings}) {return;}
            if startup {
                publish(Event::UpdateStarted);
                publish(Event::UpdateChecked(updater::check().map_err(|e|format!("{e:#}"))));
            }
            let policy=SafetyPolicy::windows();
            while let Ok(work)=commands.recv() {
                let result: anyhow::Result<Event> = (||Ok(match work {
                    Work::Refresh=>Event::Inventory(inventory::enumerate()?),
                    Work::Prepare(app)=>{
                        let inventory=inventory::enumerate()?;
                        anyhow::ensure!(inventory.warnings.is_empty(),"Inventory is incomplete; resolve the registry read errors before uninstalling");
                        let current=inventory.apps.iter().find(|a|a.id==app.id).ok_or_else(||anyhow::anyhow!("Registration no longer exists. Refresh the list"))?;
                        let windows=std::env::var("SystemRoot").unwrap_or_else(|_|"C:\\Windows".into());
                        let plan=command::plan(current,&windows)?;
                        let (snapshot,limitation)=match leftovers::capture(current,&inventory.apps,&policy) { Ok(s)=>(Some(s),None),Err(e)=>(None,Some(format!("{e:#}"))) };
                        Event::Prepared {app:current.clone(),plan,snapshot,limitation}
                    },
                    Work::Uninstall(app,approved,snapshot)=>{
                        let inventory=inventory::enumerate()?;
                        let current=inventory.apps.iter().find(|a|a.id==app.id).ok_or_else(||anyhow::anyhow!("Registration disappeared before launch"))?;
                        let windows=std::env::var("SystemRoot").unwrap_or_else(|_|"C:\\Windows".into());
                        let plan=command::plan(current,&windows)?;
                        anyhow::ensure!(plan==approved,"Registered command changed. Prepare and review it again");
                        let code=command::execute(&plan)?;
                        Event::Uninstalled {app_name:app.name,code,snapshot,inventory:inventory::enumerate()?}
                    },
                    Work::Review(snapshot)=>{
                        let inventory=inventory::enumerate()?;
                        anyhow::ensure!(inventory.warnings.is_empty(),"Cannot review leftovers with an incomplete inventory");
                        Event::Reviewed(leftovers::review(&snapshot,&inventory.apps,&policy)?,inventory)
                    },
                    Work::Quarantine(candidate)=>{
                        let data=data.as_ref().ok_or_else(||anyhow::anyhow!("Quarantine journal storage is unavailable"))?;
                        let inventory=inventory::enumerate()?;
                        anyhow::ensure!(inventory.warnings.is_empty(),"Cannot quarantine with an incomplete inventory");
                        let fresh=leftovers::review(&candidate.snapshot,&inventory.apps,&policy)?.ok_or_else(||anyhow::anyhow!("Directory is already gone"))?;
                        anyhow::ensure!(fresh.files==candidate.files,"Preview changed. Review leftovers again");
                        Event::Quarantined(leftovers::quarantine(&fresh,&data.join("quarantine"))?)
                    },
                    Work::Restore(record)=>{leftovers::restore(&record)?;Event::Restored(record.id)},
                    Work::Save(settings)=>{settings::save(data.as_ref().ok_or_else(||anyhow::anyhow!("Settings directory unavailable"))?,&settings)?;Event::Saved},
                    Work::CheckUpdate=>Event::UpdateChecked(updater::check().map_err(|e|format!("{e:#}"))),
                    Work::Download(update)=>Event::Downloaded(updater::download(&update,|done,total|{publish(Event::Progress(done,total));})?),
                    Work::Install(path)=>{updater::launch_helper(&path)?;Event::Installing},
                }))();
                let event=match result {Ok(e)=>e,Err(e)=>Event::Error(format!("{e:#}"))};
                if !publish(event) {break;}
            }
        }).expect("Could not start the bounded application worker");
        Self { tx, rx }
    }
    pub fn send(&self, work: Work) -> Result<(), String> {
        self.tx
            .try_send(work)
            .map_err(|_| "The worker is busy. Wait for the current operation".into())
    }
}
