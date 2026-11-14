use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use gpui::{Context, EventEmitter, SharedString};
use mira_ui::Tone;

use super::{Loading, RuffleView};
use crate::{
    accounts::{Account, AccountId},
    automation::sign_in::Status,
    scripting::Values,
};

pub(crate) struct Changed;

impl EventEmitter<Changed> for RuffleView {}

pub(crate) struct Identified(pub(crate) String);

impl EventEmitter<Identified> for RuffleView {}

pub(super) struct Link {
    name: String,
    username: String,
}

impl Link {
    fn new(account: &Account) -> Self {
        Self {
            name: account.name.clone(),
            username: account.username.clone(),
        }
    }

    fn id(&self) -> AccountId {
        AccountId::of(&self.username)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Busy,
    Online,
    Offline,
    Failed,
}

pub(crate) struct Glance {
    pub(crate) label: SharedString,
    pub(crate) tone: Tone,
}

impl Glance {
    fn new(label: impl Into<SharedString>, tone: Tone) -> Self {
        Self {
            label: label.into(),
            tone,
        }
    }
}

pub(crate) struct Details {
    pub(crate) server: Option<SharedString>,
    pub(crate) script: Option<SharedString>,
    pub(crate) error: Option<SharedString>,
}

pub(crate) struct Summary {
    pub(crate) state: State,
    pub(crate) login: SharedString,
    pub(crate) script: Option<SharedString>,
    pub(crate) script_failed: bool,
    pub(crate) running: bool,
}

impl RuffleView {
    pub(super) fn sign_in(
        &mut self,
        account: &Account,
        script: Option<(PathBuf, Values)>,
        cx: &mut Context<Self>,
    ) {
        self.account = Some(Link::new(account));
        if let Some((script, values)) = script {
            self.queue.add(script, values);
            self.queue.play();
        }
        self.use_account_settings(account.id(), cx);
        self.status = None;
        self.session
            .sign_in(account.credentials(), account.server.clone());
        self.changed(cx);
    }

    pub(super) fn adopt(&mut self, account: &Account, cx: &mut Context<Self>) {
        self.account = Some(Link::new(account));
        self.changed(cx);
    }

    pub(super) fn identify(&mut self, player: &str, cx: &mut Context<Self>) -> bool {
        let id = AccountId::of(player);
        if self.account.as_ref().is_some_and(|link| link.id() == id) {
            return false;
        }
        if self.account.take().is_some() {
            self.queue.pause();
        }
        cx.emit(Identified(player.to_owned()));
        true
    }

    pub(super) fn signed_in(&mut self, status: Status, cx: &mut Context<Self>) {
        self.status = Some(status);
        self.run_queued(cx);
        self.changed(cx);
    }

    pub(super) fn queue_script(
        &mut self,
        script: PathBuf,
        values: Values,
        replace: bool,
        cx: &mut Context<Self>,
    ) {
        if replace {
            self.queue.add_next(script, values);
            self.skip_script(cx);
        } else if !self.queue.waits_for(&script) {
            self.queue.add(script, values);
        }
        self.queue.play();
        self.run_queued(cx);
        self.changed(cx);
    }

    pub(super) fn run_queued(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.status, Some(Status::Online(_))) || self.script.running() {
            return;
        }
        if let Some(launch) = self.queue.start_next(Instant::now()) {
            self.start_script(launch, cx);
        }
    }

    fn waiting(&self) -> Option<&Path> {
        self.queue
            .next()
            .filter(|_| self.queue.playing() && !self.script.running())
            .map(|entry| entry.script.as_path())
    }

    pub(super) fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(Changed);
        cx.notify();
    }

    pub(crate) fn summary(&self) -> Summary {
        let status = self.status.as_ref();
        let login: SharedString = match (&self.error, status, self.loading) {
            (Some(error), _, _) => format!("Couldn't start: {error}").into(),
            (None, None, Loading::Percent(percent)) => format!("Loading {percent}%").into(),
            (None, None, _) => "Loading".into(),
            (None, Some(status), _) => login_label(status),
        };
        let queued = self
            .waiting()
            .map(|script| format!("Waiting to run {}", file_name(script)).into());
        let state = match (&self.error, status) {
            (Some(_), _) | (None, Some(Status::Failed(_))) => State::Failed,
            (None, Some(Status::Online(_))) => State::Online,
            (None, Some(Status::Offline)) => State::Offline,
            (None, _) => State::Busy,
        };
        Summary {
            state,
            login,
            script_failed: queued.is_none() && self.script.failed(),
            script: queued.or_else(|| self.script_status().map(Into::into)),
            running: self.script.running(),
        }
    }

    pub(super) fn glance(&self) -> Option<Glance> {
        let link = self.account.as_ref();
        let queued = self.waiting();
        if self.error.is_some() {
            return Some(Glance::new("Couldn't start", Tone::Danger));
        }
        let online = match self.status.as_ref() {
            Some(status @ Status::Online(_)) => Some(login_label(status)),
            Some(Status::Failed(_)) => return Some(Glance::new("Login failed", Tone::Danger)),
            Some(Status::LoggingIn { attempt: 1, .. }) => {
                return Some(Glance::new("Logging in", Tone::Accent));
            }
            Some(Status::LoggingIn { attempt, of }) => {
                return Some(Glance::new(
                    format!("Logging in {attempt}/{of}"),
                    Tone::Accent,
                ));
            }
            Some(Status::Retrying { attempt, of }) => {
                return Some(Glance::new(
                    format!("Retrying {attempt}/{of}"),
                    Tone::Warning,
                ));
            }
            Some(Status::ChooseServer) => return Some(Glance::new("Choose server", Tone::Warning)),
            Some(Status::Offline) if link.is_none() => {
                return Some(Glance::new("Not logged in", Tone::Neutral));
            }
            Some(Status::Offline) => return Some(Glance::new("Offline", Tone::Warning)),
            None => match (link, self.loading) {
                (None, Loading::Done) => None,
                (_, Loading::Percent(percent)) => {
                    return Some(Glance::new(format!("Loading {percent}%"), Tone::Neutral));
                }
                (_, _) => return Some(Glance::new("Loading", Tone::Neutral)),
            },
        };
        let script = self.script_status();
        if let Some(name) = self.script.loaded()
            && self.script.failed()
            && queued.is_none()
        {
            return Some(Glance::new(format!("{name} failed"), Tone::Danger));
        }
        if self.script.running() {
            return script.map(|status| Glance::new(status, Tone::Accent));
        }
        if let Some(queued) = queued {
            let waiting = format!("Waiting to run {}", file_name(queued));
            return Some(Glance::new(waiting, Tone::Neutral));
        }
        match script {
            Some(script) => Some(Glance::new(script, Tone::Neutral)),
            None => online.map(|online| Glance::new(online, Tone::Neutral)),
        }
    }

    pub(super) fn details(&self) -> Details {
        let status = self.status.as_ref();
        let queued = self.waiting();
        let server = match status {
            Some(Status::Online(server)) => server.clone().map(Into::into),
            _ => None,
        };
        let script = match queued {
            Some(queued) => Some(file_name(queued).into()),
            None => self.script.loaded().cloned(),
        };
        let error = match (&self.error, status) {
            (Some(error), _) | (None, Some(Status::Failed(error))) => Some(error.clone().into()),
            _ => self
                .script
                .failure()
                .filter(|_| queued.is_none())
                .map(Into::into),
        };
        Details {
            server,
            script,
            error,
        }
    }

    pub(super) fn character(&self) -> Option<&str> {
        self.bar.character()
    }

    pub(super) fn username(&self) -> Option<&str> {
        self.account.as_ref().map(|link| link.username.as_str())
    }

    pub(super) fn account_name(&self) -> Option<&str> {
        self.account.as_ref().map(|link| link.name.as_str())
    }

    pub(super) fn account_id(&self) -> Option<AccountId> {
        self.account.as_ref().map(Link::id)
    }

    pub(super) fn retry_sign_in(&mut self, account: &Account, cx: &mut Context<Self>) {
        let failed = self.account.is_some() && matches!(self.status, Some(Status::Failed(_)));
        if failed {
            self.sign_in(account, None, cx);
        }
    }
}

fn login_label(status: &Status) -> SharedString {
    match status {
        Status::LoggingIn { attempt: 1, .. } => "Logging in".into(),
        Status::LoggingIn { attempt, of } => {
            format!("Logging in, attempt {attempt} of {of}").into()
        }
        Status::Retrying { attempt, of } => {
            format!("Attempt {attempt} of {of} failed, retrying").into()
        }
        Status::ChooseServer => "Choose a server".into(),
        Status::Online(Some(server)) => server.clone().into(),
        Status::Online(None) => "Online".into(),
        Status::Offline => "Offline".into(),
        Status::Failed(error) => format!("Login failed: {error}").into(),
    }
}

pub(crate) fn file_name(path: &std::path::Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.display().to_string(),
    )
}
