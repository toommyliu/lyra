use std::{
    collections::{HashMap, HashSet},
    fs, io, iter,
    path::{Path, PathBuf},
    rc::Rc,
};

use gpui::{
    App, AppContext as _, Context, Entity, Focusable as _, FontWeight, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, PathPromptOptions, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, WeakEntity, Window, actions,
    div, prelude::FluentBuilder as _, px,
};
use mira_ui::{
    ActiveTheme as _, Theme, Tone,
    primitives::{
        Alert, Button, Checkbox, CheckedState, Dialog, Field, Input, InputEvent, List, ListItem,
        Menu, MenuItem, Popover, Radio, RadioGroup, Select, Switch, ToggleGroup, VirtualList,
        button::{Size, Variant},
    },
    theme::{CONTROL_MD, LINE_BODY, LINE_SMALL, TEXT_BODY, TEXT_SMALL},
};

use super::{
    Client, RuffleView,
    account::file_name,
    client::Tab,
    queue::{self, EntryId, Queue},
    script::failure,
    script_form::{
        Draft, FieldDraft, ROOMS, SPECIFIC_ROOM, drafts, error, room_choice, room_note, rows,
        starting, summary, values,
    },
};
use crate::{
    account_settings::{self, RoomNumber, Rooms, Scripts},
    accounts::AccountId,
    script_inputs,
    scripting::{
        self, Definition, ExtractError, Kind, Launch, Outcome, SCRIPT_EXTENSIONS, Supplied, Values,
    },
    settings,
};

actions!(mira, [OpenScripts]);

const PANE_HEIGHT: Pixels = px(332.0);
const LIBRARY_WIDTH: Pixels = px(300.0);
const SEARCH_WIDTH: Pixels = px(260.0);
const PANES_WIDTH: Pixels = px(180.0);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    path: PathBuf,
    name: SharedString,
    folder: Option<SharedString>,
    source: Option<usize>,
}

impl Entry {
    fn matches(&self, query: &str) -> bool {
        self.name.to_lowercase().contains(query)
            || self
                .folder
                .as_ref()
                .is_some_and(|folder| folder.to_lowercase().contains(query))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SourceKind {
    Folder,
    Package {
        version: Option<SharedString>,
        description: Option<SharedString>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    root: PathBuf,
    name: SharedString,
    kind: SourceKind,
    readable: bool,
    scripts: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Library {
    entries: Vec<Entry>,
    sources: Vec<Source>,
}

impl Library {
    fn add(&mut self, mut source: Source, scanned: io::Result<Vec<Entry>>) {
        let index = Some(self.sources.len());
        match scanned {
            Ok(entries) => {
                source.scripts = entries.len();
                self.entries.extend(entries.into_iter().map(|entry| Entry {
                    source: index,
                    ..entry
                }));
            }
            Err(error) => {
                tracing::warn!(%error, path = %source.root.display(), "couldn't read a script source");
                source.readable = false;
            }
        }
        self.sources.push(source);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    Source(usize),
    Script(Entry),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    All,
    BuiltIn,
    Source(usize),
}

fn library_rows(library: &Library, query: &str, scope: Scope) -> Vec<Row> {
    let scripts = |source: Option<usize>| {
        library
            .entries
            .iter()
            .filter(move |entry| entry.source == source && entry.matches(query))
            .cloned()
            .map(Row::Script)
    };
    match scope {
        Scope::BuiltIn => return scripts(None).collect(),
        Scope::Source(index) => return scripts(Some(index)).collect(),
        Scope::All => {}
    }
    let mut rows: Vec<Row> = scripts(None).collect();
    for (index, source) in library.sources.iter().enumerate() {
        let named = source.name.to_lowercase().contains(query);
        let scripts: Vec<Row> = library
            .entries
            .iter()
            .filter(|entry| entry.source == Some(index) && (named || entry.matches(query)))
            .cloned()
            .map(Row::Script)
            .collect();
        if named || !scripts.is_empty() {
            rows.push(Row::Source(index));
            rows.extend(scripts);
        }
    }
    rows
}

fn scan_all(built_in: &Path, added: &[PathBuf], packages: &Path) -> Library {
    let mut library = Library::default();
    match scan(built_in) {
        Ok(entries) => library.entries = entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, path = %built_in.display(), "couldn't read the scripts folder");
        }
    }
    for root in added {
        let name = root.file_name().map_or_else(
            || root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let source = Source {
            root: root.clone(),
            name: name.into(),
            kind: SourceKind::Folder,
            readable: true,
            scripts: 0,
        };
        library.add(source, scan(root));
    }
    for root in package_roots(packages) {
        let (source, scripts) = package(root);
        library.add(source, scripts);
    }
    library
}

fn package_roots(packages: &Path) -> Vec<PathBuf> {
    let Ok(items) = fs::read_dir(packages) else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = items
        .filter_map(Result::ok)
        .map(|item| item.path())
        .filter(|path| path.join("package.json").is_file())
        .collect();
    roots.sort();
    roots
}

fn package(root: PathBuf) -> (Source, io::Result<Vec<Entry>>) {
    let folder_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let manifest = fs::read(root.join("package.json")).and_then(|bytes| {
        serde_json::from_slice::<serde_json::Value>(&bytes).map_err(io::Error::from)
    });
    let text = |manifest: &serde_json::Value, key: &str| {
        manifest
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(|text| SharedString::from(text.to_owned()))
    };
    let (name, kind, scripts) = match manifest {
        Ok(manifest) => {
            let scripts = match scan(&root.join("scripts")) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
                scanned => scanned,
            };
            (
                text(&manifest, "name").unwrap_or_else(|| folder_name.into()),
                SourceKind::Package {
                    version: text(&manifest, "version"),
                    description: text(&manifest, "description"),
                },
                scripts,
            )
        }
        Err(error) => (
            folder_name.into(),
            SourceKind::Package {
                version: None,
                description: None,
            },
            Err(error),
        ),
    };
    let source = Source {
        root,
        name,
        kind,
        readable: true,
        scripts: 0,
    };
    (source, scripts)
}

fn scan(root: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    visit(root, root, &mut entries)?;
    entries.sort_by_cached_key(|entry| {
        (
            entry.folder.as_deref().unwrap_or_default().to_lowercase(),
            entry.name.to_lowercase(),
        )
    });
    Ok(entries)
}

fn visit(root: &Path, dir: &Path, entries: &mut Vec<Entry>) -> io::Result<()> {
    for item in fs::read_dir(dir)? {
        let item = item?;
        let name = item.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let path = item.path();
        if item.file_type()?.is_dir() {
            if let Err(error) = visit(root, &path, entries) {
                tracing::warn!(%error, path = %path.display(), "couldn't read a script folder");
            }
        } else if runnable(name) {
            let folder = dir
                .strip_prefix(root)
                .ok()
                .filter(|folder| !folder.as_os_str().is_empty())
                .map(|folder| folder.display().to_string().into());
            entries.push(Entry {
                name: name.to_owned().into(),
                folder,
                path,
                source: None,
            });
        }
    }
    Ok(())
}

fn home_relative(path: &Path) -> SharedString {
    dirs::home_dir()
        .and_then(|home| path.strip_prefix(home).ok())
        .map_or_else(
            || path.display().to_string(),
            |rest| Path::new("~").join(rest).display().to_string(),
        )
        .into()
}

fn runnable(name: &str) -> bool {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    SCRIPT_EXTENSIONS.contains(&extension) && !stem.ends_with(".d")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Library,
    Queue,
}

pub(super) struct Picker {
    client: WeakEntity<Client>,
    folder: PathBuf,
    packages: PathBuf,
    types: PathBuf,
    open: bool,
    tab: usize,
    library: Option<Library>,
    scanning: Option<Task<()>>,
    search: Entity<Input>,
    room: Entity<Input>,
    room_pending: bool,
    room_error: bool,
    selected: usize,
    filter: Option<PathBuf>,
    pane: Pane,
    queued: usize,
    chosen: Option<PathBuf>,
    forms: HashMap<FormKey, Form>,
    generation: u64,
    loading: HashMap<FormKey, Task<()>>,
    _subscriptions: [Subscription; 3],
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum FormKey {
    Script(PathBuf),
    Entry(EntryId),
}

struct Form {
    generation: u64,
    state: FormState,
}

enum FormState {
    Broken {
        title: &'static str,
        message: SharedString,
    },
    Plain,
    Fields {
        definition: Definition,
        drafts: Vec<FieldDraft>,
        texts: Vec<Option<Entity<Input>>>,
        _changes: Vec<Subscription>,
    },
}

impl Picker {
    pub(super) fn new(
        client: &Entity<Client>,
        folder: PathBuf,
        types: PathBuf,
        tab: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            Input::new("Search scripts", cx)
                .placeholder("Search")
                .search()
        });
        let room = cx.new(|cx| Input::new("Room number", cx).max_length(5));
        let subscriptions = [
            cx.observe(client, |_, _, cx| cx.notify()),
            cx.subscribe(&search, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Changed => {
                    this.pane = Pane::Library;
                    this.chosen = None;
                    this.selected = first_script(&this.rows(cx));
                    cx.notify();
                }
                InputEvent::Submitted => this.run_selected(cx),
            }),
            cx.subscribe_in(&room, window, |this, room, event, window, cx| {
                let number = room.read(cx).text().to_owned();
                match event {
                    InputEvent::Submitted if RoomNumber::parse(&number).is_some() => {
                        window.blur(cx)
                    }
                    InputEvent::Submitted => this.room_error = true,
                    InputEvent::Changed => {
                        this.room_error = room_note(&number).is_err();
                        if this.room_choice(cx) == SPECIFIC_ROOM {
                            this.choose_room_number(&number, cx);
                        }
                    }
                }
                this.show_room_error(cx);
            }),
        ];
        Self {
            client: client.downgrade(),
            packages: folder.with_file_name("packages"),
            folder,
            types,
            open: false,
            tab,
            library: None,
            scanning: None,
            search,
            room,
            room_pending: false,
            room_error: false,
            selected: 0,
            filter: None,
            pane: Pane::Library,
            queued: 0,
            chosen: None,
            forms: HashMap::new(),
            generation: 0,
            loading: HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn show(&mut self, tab: usize, account: Option<&AccountId>, cx: &mut Context<Self>) {
        self.open = true;
        self.tab = tab;
        self.pane = Pane::Library;
        self.queued = 0;
        self.chosen = None;
        self.generation += 1;
        self.search.update(cx, |search, cx| search.set_text("", cx));
        let number = match account.map(|account| account_settings::get(account, cx).scripts.rooms) {
            Some(Rooms::Specific { number }) => number.get().to_string(),
            _ => String::new(),
        };
        self.room_pending = false;
        self.room_error = false;
        self.room.update(cx, |input, cx| input.set_text(number, cx));
        self.selected = 0;
        self.rescan(cx);
        cx.notify();
    }

    fn show_room_error(&mut self, cx: &mut Context<Self>) {
        let invalid = self.room_choice(cx) == SPECIFIC_ROOM && self.room_error;
        self.room
            .update(cx, |input, cx| input.set_invalid(invalid, cx));
        cx.notify();
    }

    fn scripts(&self, cx: &App) -> Option<Scripts> {
        let account = self.tab(cx)?.account?;
        Some(account_settings::get(&account, cx).scripts)
    }

    fn change_scripts(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Scripts)) {
        if let Some(account) = self.tab(cx).and_then(|tab| tab.account) {
            account_settings::change(&account, cx, |settings| edit(&mut settings.scripts));
        }
        cx.notify();
    }

    fn room_choice(&self, cx: &App) -> usize {
        if self.room_pending {
            return SPECIFIC_ROOM;
        }
        room_choice(self.scripts(cx).unwrap_or_default().rooms)
    }

    fn choose_room(&mut self, choice: usize, window: &mut Window, cx: &mut Context<Self>) {
        let rooms = match choice {
            0 => Rooms::Public,
            1 => Rooms::RandomPrivate,
            _ => {
                let number = self.room.read(cx).text().to_owned();
                self.room_error = room_note(&number).is_err();
                self.choose_room_number(&number, cx);
                window.focus(&self.room.focus_handle(cx), cx);
                return self.show_room_error(cx);
            }
        };
        self.room_pending = false;
        self.change_scripts(cx, |scripts| scripts.rooms = rooms);
        self.show_room_error(cx);
    }

    fn choose_room_number(&mut self, number: &str, cx: &mut Context<Self>) {
        let Some(number) = RoomNumber::parse(number) else {
            self.room_pending = true;
            return;
        };
        self.room_pending = false;
        self.change_scripts(cx, |scripts| scripts.rooms = Rooms::Specific { number });
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }

    fn rescan(&mut self, cx: &mut Context<Self>) {
        let built_in = self.folder.clone();
        let added = settings::script_folders(cx).to_vec();
        let packages = self.packages.clone();
        let types = self.types.clone();
        let scanned = cx.background_spawn(async move {
            let folders = iter::once(&built_in).chain(&added);
            scripting::link_types(folders.map(PathBuf::as_path), &types);
            scan_all(&built_in, &added, &packages)
        });
        self.scanning = Some(cx.spawn(async move |this, cx| {
            let scanned = scanned.await;
            let _ = this.update(cx, |this, cx| {
                let first = this.library.is_none();
                this.library = Some(scanned);
                if first {
                    this.select_last_run(cx);
                }
                let rows = this.rows(cx);
                if !matches!(rows.get(this.selected), Some(Row::Script(_))) {
                    this.selected = first_script(&rows);
                }
                cx.notify();
            });
        }));
    }

    fn select_last_run(&mut self, cx: &App) {
        let last = self.tab(cx).and_then(|tab| tab.script);
        if let Some(index) = self
            .rows(cx)
            .iter()
            .position(|row| matches!(row, Row::Script(entry) if Some(&entry.path) == last.as_ref()))
        {
            self.selected = index;
        }
    }

    fn tab(&self, cx: &App) -> Option<Tab> {
        self.client
            .upgrade()?
            .read(cx)
            .tabs(cx)
            .into_iter()
            .find(|tab| tab.number == self.tab)
    }

    fn rows(&self, cx: &App) -> Rc<[Row]> {
        let Some(library) = &self.library else {
            return Rc::new([]);
        };
        let query = self.search.read(cx).text().trim().to_lowercase();
        library_rows(library, &query, self.scope()).into()
    }

    fn scope(&self) -> Scope {
        let Some(filter) = &self.filter else {
            return Scope::All;
        };
        if *filter == self.folder {
            return Scope::BuiltIn;
        }
        self.library
            .as_ref()
            .and_then(|library| {
                library
                    .sources
                    .iter()
                    .position(|source| source.root == *filter)
            })
            .map_or(Scope::All, Scope::Source)
    }

    fn set_filter(&mut self, filter: Option<PathBuf>, cx: &mut Context<Self>) {
        self.filter = filter;
        self.chosen = None;
        self.selected = first_script(&self.rows(cx));
        cx.notify();
    }

    fn run_selected(&mut self, cx: &mut Context<Self>) {
        if self.tab(cx).is_none_or(|tab| tab.running) || self.room_pending {
            return;
        }
        let Some((entry, values)) = self.launch(cx) else {
            return;
        };
        let launch = Launch {
            entry,
            inputs: Supplied::Values(values),
        };
        let tab = self.tab;
        let _ = self
            .client
            .update(cx, |client, cx| client.start_script_in(tab, launch, cx));
        self.close(cx);
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        let tab = self.tab;
        let _ = self
            .client
            .update(cx, |client, cx| client.stop_script_in(tab, cx));
    }

    fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        let rows = self.rows(cx);
        let mut index = self.selected;
        while let Some(next) = index
            .checked_add_signed(by)
            .filter(|next| *next < rows.len())
        {
            index = next;
            if matches!(rows[index], Row::Script(_)) {
                self.selected = index;
                self.chosen = None;
                cx.notify();
                return;
            }
        }
    }

    fn choose_file(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            if let Some(path) = paths.into_iter().next() {
                let _ = this.update(cx, |this, cx| {
                    this.pane = Pane::Library;
                    this.chosen = Some(path);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn show_folder(&mut self, folder: &Path, cx: &mut Context<Self>) {
        if folder == self.folder
            && let Err(error) = fs::create_dir_all(folder)
        {
            tracing::warn!(%error, path = %folder.display(), "couldn't create the scripts folder");
            return;
        }
        cx.open_with_system(folder);
    }

    fn add_folder(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(folder) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if folder != this.folder {
                    settings::add_script_folder(folder, cx);
                    this.rescan(cx);
                }
            });
        })
        .detach();
    }

    fn remove_folder(&mut self, folder: &Path, cx: &mut Context<Self>) {
        settings::remove_script_folder(folder, cx);
        self.rescan(cx);
    }

    fn locate_folder(&mut self, old: PathBuf, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(folder) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if folder != this.folder {
                    settings::replace_script_folder(&old, folder, cx);
                    this.rescan(cx);
                }
            });
        })
        .detach();
    }

    fn sources(&self, cx: &mut Context<Self>) -> Menu {
        let this = cx.entity().downgrade();
        let act =
            |label: &'static str, root: &Path, f: fn(&mut Self, PathBuf, &mut Context<Self>)| {
                let this = this.clone();
                let root = root.to_owned();
                MenuItem::action(label, move |_, cx| {
                    let _ = this.update(cx, |this, cx| f(this, root.clone(), cx));
                })
            };
        let show = |root: &Path| {
            act("Show in Finder", root, |this, root, cx| {
                this.show_folder(&root, cx)
            })
        };
        let remove = |root: &Path| {
            act("Remove from library", root, |this, root, cx| {
                this.remove_folder(&root, cx)
            })
        };
        let locate = |root: &Path| act("Locate…", root, Self::locate_folder);
        let sources = self
            .library
            .as_ref()
            .map_or(&[][..], |library| library.sources.as_slice());
        let mut items = vec![
            MenuItem::section("Folders"),
            MenuItem::submenu(home_relative(&self.folder), vec![show(&self.folder)]),
        ];
        let mut packages = Vec::new();
        for source in sources {
            match &source.kind {
                SourceKind::Folder => {
                    let path = home_relative(&source.root);
                    let item = if source.readable {
                        MenuItem::submenu(path, vec![show(&source.root), remove(&source.root)])
                    } else {
                        MenuItem::submenu(
                            format!("{path} (can't read)"),
                            vec![locate(&source.root), remove(&source.root)],
                        )
                    };
                    items.push(item);
                }
                SourceKind::Package { version, .. } => {
                    let label: SharedString = match version {
                        Some(version) => format!("{} {version}", source.name).into(),
                        None => source.name.clone(),
                    };
                    packages.push(MenuItem::submenu(
                        label,
                        vec![
                            show(&source.root),
                            MenuItem::action("Update", |_, _| {}).disabled(true),
                            MenuItem::action("Move to Trash", |_, _| {}).disabled(true),
                        ],
                    ));
                }
            }
        }
        let add = this.clone();
        items.push(MenuItem::action("Add folder…", move |_, cx| {
            let _ = add.update(cx, |this, cx| this.add_folder(cx));
        }));
        items.push(MenuItem::separator());
        items.push(MenuItem::section("Packages"));
        items.extend(packages);
        items.push(MenuItem::action("Install package…", |_, _| {}).disabled(true));
        items.push(MenuItem::action("Check for updates", |_, _| {}).disabled(true));
        Menu::new("sources", "Sources", items)
    }

    fn set_pane(&mut self, pane: Pane, cx: &mut Context<Self>) {
        self.pane = pane;
        cx.notify();
    }

    fn target(&self, cx: &App) -> Option<Entry> {
        if let Some(path) = &self.chosen {
            return Some(Entry {
                name: file_name(path).into(),
                path: path.clone(),
                folder: None,
                source: None,
            });
        }
        let rows = self.rows(cx);
        match rows.get(self.selected.min(rows.len().saturating_sub(1))) {
            Some(Row::Script(entry)) => Some(entry.clone()),
            _ => None,
        }
    }

    fn load_form(&mut self, key: FormKey, path: &Path, cx: &mut Context<Self>) {
        let current = self
            .forms
            .get(&key)
            .is_some_and(|form| form.generation == self.generation);
        let pending = self.loading.contains_key(&key);
        if current || pending {
            return;
        }
        let script = path.to_owned();
        let read = cx.background_spawn(async move {
            let display = file_name(&script);
            let text = fs::read_to_string(&script).map_err(|error| {
                (
                    "This script can't be read",
                    format!("Couldn't read {display}: {error}"),
                )
            })?;
            scripting::extract(&display, &script, &text).map_err(|error| match error {
                ExtractError::Syntax(message) => ("This script has a syntax error", message),
                ExtractError::Inputs(message) => ("This script's inputs are invalid", message),
            })
        });
        let generation = self.generation;
        let task = cx.spawn({
            let key = key.clone();
            async move |this, cx| {
                let read = read.await;
                let _ = this.update(cx, |this, cx| {
                    this.loading.remove(&key);
                    this.loaded(key, generation, read, cx);
                    cx.notify();
                });
            }
        });
        self.loading.insert(key, task);
    }

    fn loaded(
        &mut self,
        key: FormKey,
        generation: u64,
        read: Result<Option<Definition>, (&'static str, String)>,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.forms.get_mut(&key)
            && let (FormState::Fields { definition, .. }, Ok(Some(read))) = (&form.state, &read)
            && definition == read
        {
            form.generation = generation;
            return;
        }
        let state = match read {
            Err((title, message)) => FormState::Broken {
                title,
                message: message.into(),
            },
            Ok(None) => FormState::Plain,
            Ok(Some(definition)) => {
                let saved = match &key {
                    FormKey::Script(path) => match script_inputs::saved(path, cx) {
                        Supplied::Values(values) => values,
                        Supplied::StoreUnreadable => Values::new(),
                    },
                    FormKey::Entry(id) => self.entry_values(*id, cx).unwrap_or_default(),
                };
                let drafts = drafts(&definition, &saved);
                let mut texts = Vec::new();
                let mut changes = Vec::new();
                for (index, (field, draft)) in definition.fields().iter().zip(&drafts).enumerate() {
                    let Draft::Text(text) = &draft.draft else {
                        texts.push(None);
                        continue;
                    };
                    let invalid = error(field, draft).is_some();
                    let label = field.label.clone();
                    let text = text.clone();
                    let input = cx.new(|cx| {
                        let mut input = Input::new(label, cx);
                        input.set_text(text, cx);
                        input.set_invalid(invalid, cx);
                        input
                    });
                    let key = key.clone();
                    changes.push(cx.subscribe(
                        &input,
                        move |this, input, event: &InputEvent, cx| {
                            if matches!(event, InputEvent::Changed) {
                                let text = input.read(cx).text().to_owned().into();
                                this.set_draft(&key, index, Draft::Text(text), cx);
                            }
                        },
                    ));
                    texts.push(Some(input));
                }
                FormState::Fields {
                    definition,
                    drafts,
                    texts,
                    _changes: changes,
                }
            }
        };
        let checked = match (&key, &state) {
            (
                FormKey::Entry(_),
                FormState::Fields {
                    definition, drafts, ..
                },
            ) => Some(values(definition, drafts)),
            _ => None,
        };
        self.forms.insert(key.clone(), Form { generation, state });
        if let Some(edited) = checked {
            self.store_entry(&key, edited, cx);
        }
    }

    fn set_draft(&mut self, key: &FormKey, index: usize, draft: Draft, cx: &mut Context<Self>) {
        let Some(Form {
            state:
                FormState::Fields {
                    definition,
                    drafts,
                    texts,
                    ..
                },
            ..
        }) = self.forms.get_mut(key)
        else {
            return;
        };
        let (Some(field), Some(slot)) = (definition.fields().get(index), drafts.get_mut(index))
        else {
            return;
        };
        *slot = FieldDraft { draft, stale: None };
        if let Some(Some(input)) = texts.get(index) {
            let invalid = error(field, slot).is_some();
            input.update(cx, |input, cx| input.set_invalid(invalid, cx));
        }
        let edited = values(definition, drafts);
        self.store_entry(key, edited, cx);
        cx.notify();
    }

    fn store_entry(&mut self, key: &FormKey, edited: Option<Values>, cx: &mut Context<Self>) {
        if let FormKey::Entry(id) = key {
            let id = *id;
            self.edit_queue(cx, |queue| queue.set_values(id, edited));
        }
    }

    fn entry_values(&self, id: EntryId, cx: &App) -> Option<Values> {
        let view = self.view(cx)?;
        let queue = &view.read(cx).queue;
        let entry = queue.entries().iter().find(|entry| entry.id == id)?;
        entry.values.clone()
    }

    fn reset_form(&mut self, key: &FormKey, cx: &mut Context<Self>) {
        let Some(Form {
            state:
                FormState::Fields {
                    definition,
                    drafts,
                    texts,
                    ..
                },
            ..
        }) = self.forms.get_mut(key)
        else {
            return;
        };
        for ((field, draft), input) in definition.fields().iter().zip(drafts.iter_mut()).zip(texts)
        {
            *draft = FieldDraft {
                draft: starting(field),
                stale: None,
            };
            if let (Some(input), Draft::Text(text)) = (input, &draft.draft) {
                let invalid = error(field, draft).is_some();
                let text = text.clone();
                input.update(cx, |input, cx| {
                    input.set_text(text, cx);
                    input.set_invalid(invalid, cx);
                });
            }
        }
        let edited = values(definition, drafts);
        self.store_entry(key, edited, cx);
        cx.notify();
    }

    fn launch_values(&self, path: &Path, cx: &App) -> Option<(Option<&Definition>, Values)> {
        match &self.forms.get(&FormKey::Script(path.to_owned()))?.state {
            FormState::Broken { .. } => None,
            FormState::Plain => Some((None, Values::new())),
            FormState::Fields {
                definition, drafts, ..
            } => {
                if script_inputs::unreadable(cx) {
                    return None;
                }
                Some((Some(definition), values(definition, drafts)?))
            }
        }
    }

    fn launch(&mut self, cx: &mut Context<Self>) -> Option<(PathBuf, Values)> {
        let entry = self.target(cx)?;
        let (definition, values) = self.launch_values(&entry.path, cx)?;
        if definition.is_some() {
            script_inputs::save(&entry.path, values.clone(), cx);
        }
        Some((entry.path, values))
    }

    fn view(&self, cx: &App) -> Option<Entity<RuffleView>> {
        self.client.upgrade()?.read(cx).view_in(self.tab)
    }

    fn entry_ready(&self, id: EntryId) -> bool {
        match self.forms.get(&FormKey::Entry(id)).map(|form| &form.state) {
            Some(FormState::Plain) => true,
            Some(FormState::Fields {
                definition, drafts, ..
            }) => values(definition, drafts).is_some(),
            Some(FormState::Broken { .. }) | None => false,
        }
    }

    fn queue(&self, cx: &App) -> (Vec<queue::Entry>, bool) {
        self.view(cx).map_or_else(Default::default, |view| {
            let queue = &view.read(cx).queue;
            (queue.entries().to_vec(), queue.playing())
        })
    }

    fn queued_id(&self, index: usize, cx: &App) -> Option<EntryId> {
        let view = self.view(cx)?;
        let entry = view.read(cx).queue.entries().get(index)?;
        Some(entry.id)
    }

    fn edit_queue(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Queue)) {
        if let Some(view) = self.view(cx) {
            view.update(cx, |view, cx| view.edit_queue(cx, edit));
        }
        cx.notify();
    }

    fn add_to_queue(&mut self, cx: &mut Context<Self>) {
        let Some((script, values)) = self.launch(cx) else {
            return;
        };
        self.edit_queue(cx, |queue| {
            queue.add(script, values);
        });
    }

    fn move_queued(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let Some(id) = self.queued_id(from, cx) else {
            return;
        };
        self.edit_queue(cx, |queue| queue.move_entry(id, to));
        if let Some(index) = self.queue(cx).0.iter().position(|entry| entry.id == id) {
            self.queued = index;
        }
    }

    fn remove_queued(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.queued_id(self.queued, cx) {
            self.edit_queue(cx, |queue| queue.remove(id));
        }
    }

    fn act_on_tab(
        &mut self,
        cx: &mut Context<Self>,
        act: fn(&mut RuffleView, &mut Context<RuffleView>),
    ) {
        if let Some(view) = self.view(cx) {
            view.update(cx, act);
        }
        cx.notify();
    }

    fn options(&self, cx: &mut Context<Self>) -> Popover {
        let this = cx.entity().downgrade();
        Popover::new("options", "Options", move |_, cx| {
            let Some(picker) = this.upgrade() else {
                return div().into_any_element();
            };
            let picker = picker.read(cx);
            let scripts = picker.scripts(cx);
            let editable = scripts.is_some();
            let scripts = scripts.unwrap_or_default();
            let room = picker.room_choice(cx);
            let room_error = picker.room_error;
            let note = room_note(picker.room.read(cx).text());
            let room_input = picker.room.clone();
            let choose_room = this.clone();
            let toggle = |set: fn(&mut Scripts, bool)| {
                let this = this.clone();
                move |on, _: &mut Window, cx: &mut App| {
                    let _ = this.update(cx, |this, cx| {
                        this.change_scripts(cx, |scripts| set(scripts, on));
                    });
                }
            };
            let theme = cx.theme().clone();
            let (Ok(hint) | Err(hint)) = note;
            div()
                .flex()
                .flex_col()
                .gap(px(14.0))
                .when(!editable, |options| {
                    options.child(muted(&theme, "Log in to change script settings."))
                })
                .child(option_row(
                    &theme,
                    "Safe start and stop",
                    "Best effort attempt to start or stop the script at your house.",
                    Switch::new("safe-start", "Safe start and stop", scripts.safe_start_stop)
                        .hide_label()
                        .disabled(!editable)
                        .on_toggle(toggle(|scripts, on| scripts.safe_start_stop = on)),
                ))
                .child(option_row(
                    &theme,
                    "Restart after reconnecting",
                    "Restart the active script when the same account reconnects.",
                    Switch::new(
                        "restart",
                        "Restart after reconnecting",
                        scripts.restart_after_reconnect,
                    )
                    .hide_label()
                    .disabled(!editable)
                    .on_toggle(toggle(|scripts, on| scripts.restart_after_reconnect = on)),
                ))
                .child(div().h(px(1.0)).bg(theme.hairline))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(option_text(
                            &theme,
                            "Rooms",
                            "Choose which rooms scripts join.",
                        ))
                        .child(
                            RadioGroup::new(
                                "rooms",
                                "Rooms",
                                ROOMS
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, label)| {
                                        let radio = Radio::new(label);
                                        let radio = if editable { radio } else { radio.disabled() };
                                        match index {
                                            SPECIFIC_ROOM if room == SPECIFIC_ROOM => radio
                                                .description(hint)
                                                .invalid(room_error)
                                                .accessory(
                                                    div().w(px(80.0)).child(room_input.clone()),
                                                ),
                                            SPECIFIC_ROOM => radio.description(hint),
                                            _ => radio,
                                        }
                                    })
                                    .collect(),
                                Some(room),
                            )
                            .on_select(move |room, window, cx| {
                                let _ = choose_room
                                    .update(cx, |this, cx| this.choose_room(room, window, cx));
                            }),
                        ),
                )
                .into_any_element()
        })
        .width(px(400.0))
    }

    fn inputs(&self, theme: &Theme, key: &FormKey, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(form) = self.forms.get(key) else {
            return muted(theme, "Reading inputs…").into_any_element();
        };
        let (definition, drafts, texts) = match &form.state {
            FormState::Broken { title, message } => {
                return Alert::new("script-broken", Tone::Danger, *title)
                    .body(message.clone())
                    .into_any_element();
            }
            FormState::Plain => {
                return muted(theme, "This script has no inputs.").into_any_element();
            }
            FormState::Fields {
                definition,
                drafts,
                texts,
                ..
            } => (definition, drafts, texts),
        };
        let this = cx.entity().downgrade();
        let saved = matches!(key, FormKey::Script(_));
        let unreadable = (saved && script_inputs::unreadable(cx)).then(|| {
            let this = this.clone();
            Alert::new(
                "inputs-unreadable",
                Tone::Warning,
                "Saved inputs couldn't be read",
            )
            .body("Start fresh to back up the file and use each script's defaults.")
            .action(
                Button::new("inputs-start-fresh", "Start fresh")
                    .size(Size::Small)
                    .on_click(move |_, _, cx| {
                        script_inputs::start_fresh(cx);
                        let _ = this.update(cx, |_, cx| cx.notify());
                    }),
            )
        });
        let stale = drafts.iter().any(|draft| draft.stale.is_some()).then(|| {
            let this = this.clone();
            let key = key.clone();
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(muted(
                    theme,
                    if saved {
                        "Some saved values don't fit this script anymore."
                    } else {
                        "Some queued values don't fit this script anymore."
                    },
                ))
                .child(
                    Button::new("inputs-reset", "Reset")
                        .size(Size::Small)
                        .on_click(move |_, _, cx| {
                            let _ = this.update(cx, |this, cx| this.reset_form(&key, cx));
                        }),
                )
        });
        let fields =
            definition
                .fields()
                .iter()
                .zip(drafts)
                .enumerate()
                .map(|(index, (field, draft))| {
                    let set = {
                        let this = this.clone();
                        let key = key.clone();
                        move |draft: Draft, cx: &mut App| {
                            let _ =
                                this.update(cx, |this, cx| this.set_draft(&key, index, draft, cx));
                        }
                    };
                    let problem = error(field, draft);
                    let control = match (&field.kind, &draft.draft, &texts[index]) {
                        (Kind::Boolean { .. }, Draft::Toggle(on), _) => {
                            let switch = Switch::new(("input", index), field.label.clone(), *on)
                                .on_toggle(move |on, _, cx| set(Draft::Toggle(on), cx));
                            let switch = match &field.description {
                                Some(description) => switch.description(description.clone()),
                                None => switch,
                            };
                            return div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(switch)
                                .when_some(problem, |slot, problem| {
                                    slot.child(error_text(theme, problem))
                                })
                                .into_any_element();
                        }
                        (Kind::Select { options, .. }, Draft::Choice(chosen), _) => Select::new(
                            ("input", index),
                            field.label.clone(),
                            options.iter().cloned().map(Into::into).collect(),
                            *chosen,
                        )
                        .placeholder("Choose…")
                        .width(px(200.0))
                        .on_select(move |chosen, _, cx| set(Draft::Choice(Some(chosen)), cx))
                        .into_any_element(),
                        (Kind::MultiSelect { options, .. }, Draft::Choices(chosen), _) => {
                            let set = Rc::new(set);
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.0))
                                .children(options.iter().enumerate().map(|(option, label)| {
                                    let set = set.clone();
                                    let chosen = chosen.clone();
                                    let state = if chosen[option] {
                                        CheckedState::Checked
                                    } else {
                                        CheckedState::Unchecked
                                    };
                                    Checkbox::new(
                                        SharedString::from(format!("input-{index}-{option}")),
                                        label.clone(),
                                        state,
                                    )
                                    .on_toggle(
                                        move |on, _, cx| {
                                            let mut chosen = chosen.clone();
                                            chosen[option] = on;
                                            set(Draft::Choices(chosen), cx);
                                        },
                                    )
                                }))
                                .into_any_element()
                        }
                        (kind, _, Some(input)) => div()
                            .when(matches!(kind, Kind::Number { .. }), |slot| {
                                slot.w(px(120.0))
                            })
                            .child(input.clone())
                            .into_any_element(),
                        _ => div().into_any_element(),
                    };
                    Field::new(field.label.clone(), control)
                        .when_some(field.description.clone(), |field, description| {
                            field.description(description)
                        })
                        .error(problem)
                        .into_any_element()
                });
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .children(unreadable)
            .children(stale)
            .children(fields)
            .into_any_element()
    }

    fn script_detail(
        &self,
        theme: &Theme,
        entry: Option<&Entry>,
        can_queue: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let inputs = self.inputs(theme, &FormKey::Script(entry.path.clone()), cx);
        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(heading(
                theme,
                entry.name.clone(),
                entry.path.display().to_string().into(),
                Button::new("queue", "Add to queue")
                    .size(Size::Small)
                    .disabled(!can_queue)
                    .on_click(cx.listener(|this, _, _, cx| this.add_to_queue(cx))),
            ))
            .child(inputs)
            .into_any_element()
    }

    fn queued_detail(
        &self,
        theme: &Theme,
        queue: &[queue::Entry],
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(queued) = queue.get(self.queued) else {
            return div().into_any_element();
        };
        let position: SharedString = match &queued.state {
            queue::State::Waiting => format!(
                "{} of {} waiting",
                queue[..self.queued]
                    .iter()
                    .filter(|entry| entry.waiting())
                    .count()
                    + 1,
                queue.iter().filter(|entry| entry.waiting()).count()
            )
            .into(),
            queue::State::Running { .. } => "Running now".into(),
            queue::State::Ended { outcome, took } => {
                format!("{} after {}", ended(outcome), duration(*took)).into()
            }
        };
        let failed = match &queued.state {
            queue::State::Ended { outcome, .. } => failure(outcome),
            _ => None,
        };
        let key = FormKey::Entry(queued.id);
        let body = match self.forms.get(&key).map(|form| &form.state) {
            _ if queued.waiting() => self.inputs(theme, &key, cx),
            None => muted(theme, "Reading inputs…").into_any_element(),
            Some(FormState::Broken { title, message }) => {
                Alert::new("queued-broken", Tone::Danger, *title)
                    .body(message.clone())
                    .into_any_element()
            }
            Some(FormState::Plain) => muted(theme, "This script has no inputs.").into_any_element(),
            Some(FormState::Fields { definition, .. }) => {
                let rows = queued
                    .values
                    .as_ref()
                    .map(|values| rows(definition, values))
                    .unwrap_or_default();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .children(rows.into_iter().map(|(label, value)| {
                        div()
                            .flex()
                            .gap(px(12.0))
                            .text_size(TEXT_BODY)
                            .line_height(LINE_BODY)
                            .child(
                                div()
                                    .w(px(140.0))
                                    .flex_none()
                                    .truncate()
                                    .text_color(theme.text_secondary)
                                    .child(label),
                            )
                            .child(div().flex_1().min_w_0().child(value))
                    }))
                    .into_any_element()
            }
        };
        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(heading(
                theme,
                file_name(&queued.script).into(),
                position,
                Button::new("queued-remove", "Remove")
                    .size(Size::Small)
                    .disabled(queued.running())
                    .on_click(cx.listener(|this, _, _, cx| this.remove_queued(cx))),
            ))
            .when_some(failed, |detail, message| {
                detail.child(Alert::new("queued-failed", Tone::Danger, "Failed").body(message))
            })
            .child(body)
            .into_any_element()
    }

    fn queue_list(
        &self,
        theme: &Theme,
        queue: &[queue::Entry],
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if queue.is_empty() {
            return div()
                .h(PANE_HEIGHT)
                .p(px(12.0))
                .text_size(TEXT_SMALL)
                .line_height(LINE_SMALL)
                .text_color(theme.text_secondary)
                .child("Nothing queued. Pick a script in Library and choose Add to queue.")
                .into_any_element();
        }
        let items = queue
            .iter()
            .map(|queued| {
                let item = ListItem::new(file_name(&queued.script));
                let form = self
                    .forms
                    .get(&FormKey::Entry(queued.id))
                    .map(|form| &form.state);
                let summary = match form {
                    Some(FormState::Fields { definition, .. }) => queued
                        .values
                        .as_ref()
                        .map(|values| summary(definition, values))
                        .unwrap_or_default(),
                    _ => SharedString::default(),
                };
                let item = if summary.is_empty() {
                    item
                } else {
                    item.detail(summary)
                };
                match &queued.state {
                    queue::State::Waiting => match form {
                        Some(FormState::Broken { .. }) => item.meta("Can't start"),
                        Some(FormState::Fields {
                            definition, drafts, ..
                        }) if values(definition, drafts).is_none() => item.meta("Needs inputs"),
                        _ => item,
                    },
                    queue::State::Running { .. } => item.meta("Running"),
                    queue::State::Ended { outcome, took } => {
                        item.meta(format!("{} · {}", ended(outcome), duration(*took)))
                    }
                }
            })
            .collect();
        let pinned = queue.iter().filter(|entry| !entry.waiting()).count();
        let this = cx.entity().downgrade();
        let that = this.clone();
        List::new("queue", "Queue", items, Some(self.queued))
            .height(PANE_HEIGHT)
            .on_select(move |index, _, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.queued = index;
                    cx.notify();
                });
            })
            .on_reorder(move |from, to, _, cx| {
                let _ = that.update(cx, |this, cx| this.move_queued(from, to, cx));
            })
            .pin_first(pinned)
            .into_any_element()
    }

    fn library(&self, theme: &Theme, rows: Rc<[Row]>, cx: &mut Context<Self>) -> gpui::AnyElement {
        let scope = self.scope();
        let filtered = self
            .library
            .as_ref()
            .is_some_and(|library| !library.sources.is_empty());
        let height = if filtered {
            PANE_HEIGHT - CONTROL_MD - px(8.0)
        } else {
            PANE_HEIGHT
        };
        let note = || {
            div()
                .h(height)
                .p(px(12.0))
                .flex()
                .flex_col()
                .gap(px(10.0))
                .text_size(TEXT_SMALL)
                .line_height(LINE_SMALL)
                .text_color(theme.text_secondary)
        };
        let Some(library) = &self.library else {
            return note().into_any_element();
        };
        if library.entries.is_empty() && library.sources.is_empty() {
            let folder = self.folder.clone();
            return note()
                .child(
                    div()
                        .text_size(TEXT_BODY)
                        .line_height(LINE_BODY)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child("No scripts yet"),
                )
                .child(format!(
                    "Put .ts or .js files in {}, or add a folder you already have.",
                    home_relative(&self.folder)
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(
                            Button::new("empty-show", "Show in Finder")
                                .size(Size::Small)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        this.show_folder(&folder, cx)
                                    }),
                                ),
                        )
                        .child(
                            Button::new("empty-add", "Add folder…")
                                .size(Size::Small)
                                .on_click(cx.listener(|this, _, _, cx| this.add_folder(cx))),
                        ),
                )
                .into_any_element();
        }
        let list = if rows.is_empty() {
            let query = self.search.read(cx).text().trim().to_owned();
            let text = if query.is_empty() {
                "No scripts here yet.".to_owned()
            } else {
                format!("No scripts match \u{201c}{query}\u{201d}.")
            };
            note().child(text).into_any_element()
        } else {
            let sources: Rc<[Source]> = library.sources.clone().into();
            let count = rows.len();
            let this = cx.entity().downgrade();
            let that = this.clone();
            VirtualList::new("scripts", "Scripts", count, move |index| {
                match &rows[index] {
                    Row::Source(source) => {
                        let source = &sources[*source];
                        ListItem::new(source.name.clone())
                            .meta(source_meta(source))
                            .disabled()
                    }
                    Row::Script(entry) => match &entry.folder {
                        Some(folder) => ListItem::new(entry.name.clone()).meta(folder.clone()),
                        None => ListItem::new(entry.name.clone()),
                    },
                }
            })
            .selected(Some(self.selected.min(count - 1)))
            .height(height)
            .on_select(move |index, _, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.selected = index;
                    this.chosen = None;
                    cx.notify();
                });
            })
            .on_activate(move |index, _, cx| {
                let _ = that.update(cx, |this, cx| {
                    this.selected = index;
                    this.chosen = None;
                    this.run_selected(cx);
                });
            })
            .into_any_element()
        };
        if !filtered {
            return list;
        }
        let mut options: Vec<SharedString> = vec!["All sources".into(), "My scripts".into()];
        let mut filters = vec![None, Some(self.folder.clone())];
        for source in &library.sources {
            options.push(source.name.clone());
            filters.push(Some(source.root.clone()));
        }
        let chosen = match scope {
            Scope::All => 0,
            Scope::BuiltIn => 1,
            Scope::Source(index) => index + 2,
        };
        let this = cx.entity().downgrade();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                Select::new("filter", "Show", options, Some(chosen))
                    .width(LIBRARY_WIDTH)
                    .on_select(move |index, _, cx| {
                        let filter = filters[index].clone();
                        let _ = this.update(cx, |this, cx| this.set_filter(filter, cx));
                    }),
            )
            .child(list)
            .into_any_element()
    }
}

fn first_script(rows: &[Row]) -> usize {
    rows.iter()
        .position(|row| matches!(row, Row::Script(_)))
        .unwrap_or(0)
}

fn source_meta(source: &Source) -> SharedString {
    if !source.readable {
        return "Can't read".into();
    }
    match &source.kind {
        SourceKind::Package {
            version: Some(version),
            ..
        } => version.clone(),
        _ => source.scripts.to_string().into(),
    }
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tab = self.tab(cx);
        let rows = self.rows(cx);
        let chosen = self.target(cx);
        if self.open
            && let Some(entry) = &chosen
        {
            self.load_form(FormKey::Script(entry.path.clone()), &entry.path, cx);
        }
        let (queue, playing) = self.queue(cx);
        let queued: HashSet<EntryId> = self
            .client
            .upgrade()
            .map(|client| client.read(cx).views())
            .unwrap_or_default()
            .iter()
            .flat_map(|view| view.read(cx).queue.entries().iter().map(|entry| entry.id))
            .collect();
        self.forms.retain(|key, _| match key {
            FormKey::Script(_) => true,
            FormKey::Entry(id) => queued.contains(id),
        });
        if self.pane == Pane::Queue {
            for entry in &queue {
                self.load_form(FormKey::Entry(entry.id), &entry.script, cx);
            }
        }
        self.queued = self.queued.min(queue.len().saturating_sub(1));
        let valid = chosen
            .as_ref()
            .is_some_and(|entry| self.launch_values(&entry.path, cx).is_some());
        let running = tab.as_ref().is_some_and(|tab| tab.running);
        let unfinished = queue
            .iter()
            .filter(|entry| !matches!(entry.state, queue::State::Ended { .. }))
            .count();
        let waiting = queue.iter().any(queue::Entry::waiting);
        let startable = queue
            .iter()
            .filter(|entry| entry.waiting())
            .all(|entry| self.entry_ready(entry.id));
        let finished = queue.len() > unfinished;
        let skippable = queue.iter().any(queue::Entry::running);
        let room_ready = !self.room_pending;
        let status: SharedString = tab
            .as_ref()
            .filter(|tab| tab.running)
            .and_then(|tab| tab.status.clone())
            .unwrap_or_else(|| "Nothing running".into());
        let search = div()
            .w(SEARCH_WIDTH)
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                let by = match event.keystroke.key.as_str() {
                    "up" => -1,
                    "down" => 1,
                    _ => return,
                };
                this.step(by, cx);
                cx.stop_propagation();
            }))
            .child(self.search.clone());
        let this = cx.entity().downgrade();
        let panes = ToggleGroup::new(
            "pane",
            "Show",
            vec![
                "Library".into(),
                if unfinished == 0 {
                    "Queue".into()
                } else {
                    format!("Queue ({unfinished})").into()
                },
            ],
            match self.pane {
                Pane::Library => 0,
                Pane::Queue => 1,
            },
        )
        .full_width()
        .on_select(move |index, _, cx| {
            let pane = if index == 0 {
                Pane::Library
            } else {
                Pane::Queue
            };
            let _ = this.update(cx, |this, cx| this.set_pane(pane, cx));
        });
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(div().w(PANES_WIDTH).child(panes))
            .when(self.pane == Pane::Library, |header| header.child(search));
        let (list, detail) = match self.pane {
            Pane::Library => (
                self.library(&theme, rows, cx),
                self.script_detail(&theme, chosen.as_ref(), tab.is_some() && valid, cx),
            ),
            Pane::Queue => (
                self.queue_list(&theme, &queue, cx),
                self.queued_detail(&theme, &queue, cx),
            ),
        };
        let body = div()
            .flex()
            .gap(px(20.0))
            .child(div().flex_none().w(LIBRARY_WIDTH).child(list))
            .child(
                div()
                    .id("detail")
                    .flex_1()
                    .min_w_0()
                    .h(PANE_HEIGHT)
                    .overflow_y_scroll()
                    .child(detail),
            );
        let close = cx.listener(|this, _: &(), _, cx| this.close(cx));
        let dialog = Dialog::new("scripts", self.open, "Scripts")
            .width(px(660.0))
            .description(status)
            .accessory(header)
            .body(body);
        let dialog = match self.pane {
            Pane::Library => dialog
                .action(
                    div()
                        .flex_1()
                        .flex()
                        .gap(px(8.0))
                        .child(self.options(cx))
                        .child(self.sources(cx))
                        .child(
                            Button::new("choose-file", "Choose file…")
                                .variant(Variant::Ghost)
                                .disabled(tab.is_none() || running)
                                .on_click(cx.listener(|this, _, _, cx| this.choose_file(cx))),
                        ),
                )
                .action(if running {
                    Button::new("stop", "Stop")
                        .on_click(cx.listener(|this, _, _, cx| this.stop(cx)))
                } else {
                    Button::new("run", "Run")
                        .variant(Variant::Primary)
                        .disabled(tab.is_none() || chosen.is_none() || !valid || !room_ready)
                        .on_click(cx.listener(|this, _, _, cx| this.run_selected(cx)))
                }),
            Pane::Queue => dialog
                .action(
                    div()
                        .flex_1()
                        .flex()
                        .gap(px(8.0))
                        .child(self.options(cx))
                        .child(
                            Button::new("queue-clear", "Clear queue")
                                .variant(Variant::Ghost)
                                .disabled(queue.len() == usize::from(skippable))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.edit_queue(cx, Queue::clear);
                                })),
                        )
                        .child(
                            Button::new("queue-clear-finished", "Clear finished")
                                .variant(Variant::Ghost)
                                .disabled(!finished)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.edit_queue(cx, Queue::clear_finished);
                                })),
                        ),
                )
                .when(playing && skippable, |dialog| {
                    dialog.action(Button::new("queue-skip", "Skip").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.act_on_tab(cx, RuffleView::skip_script);
                        },
                    )))
                })
                .action(if playing || skippable {
                    Button::new("queue-stop", "Stop queue").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.act_on_tab(cx, RuffleView::stop_script);
                        },
                    ))
                } else {
                    Button::new("queue-start", "Start queue")
                        .variant(Variant::Primary)
                        .disabled(tab.is_none() || !waiting || !startable || !room_ready)
                        .on_click(cx.listener(|this, _, _, cx| this.edit_queue(cx, Queue::play)))
                }),
        };
        dialog.on_close(move |window, cx| close(&(), window, cx))
    }
}

fn heading(
    theme: &Theme,
    name: SharedString,
    detail: SharedString,
    action: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name),
                )
                .child(action),
        )
        .child(
            div()
                .truncate()
                .text_ellipsis_start()
                .text_size(TEXT_SMALL)
                .line_height(LINE_SMALL)
                .text_color(theme.text_secondary)
                .child(detail),
        )
}

fn option_text(theme: &Theme, label: &'static str, description: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .text_size(TEXT_BODY)
                .line_height(LINE_BODY)
                .child(label),
        )
        .child(
            div()
                .text_size(TEXT_SMALL)
                .line_height(LINE_SMALL)
                .text_color(theme.text_tertiary)
                .child(description),
        )
}

fn option_row(
    theme: &Theme,
    label: &'static str,
    description: &'static str,
    control: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap(px(16.0))
        .child(option_text(theme, label, description))
        .child(div().flex_none().pt(px(1.0)).child(control))
}

fn error_text(theme: &Theme, text: SharedString) -> impl IntoElement {
    div()
        .text_size(TEXT_SMALL)
        .line_height(LINE_SMALL)
        .text_color(theme.danger_text)
        .child(text)
}

fn muted(theme: &Theme, text: &'static str) -> impl IntoElement {
    div()
        .text_size(TEXT_BODY)
        .line_height(LINE_BODY)
        .text_color(theme.text_tertiary)
        .child(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_lists_runnable_scripts_grouped_by_source() {
        let mut random = [0; 8];
        getrandom::getrandom(&mut random).unwrap();
        let temp =
            std::env::temp_dir().join(format!("mira-scripts-{:x}", u64::from_ne_bytes(random)));
        let built_in = temp.join("scripts");
        let community = temp.join("community");
        let missing = temp.join("unplugged");
        let packages = temp.join("packages");
        assert_eq!(scan_all(&built_in, &[], &packages), Library::default());
        for (file, contents) in [
            ("scripts/zeta.ts", ""),
            ("scripts/Alpha.js", ""),
            ("scripts/notes.txt", ""),
            ("scripts/types.d.ts", ""),
            ("scripts/farming/daily.mts", ""),
            ("scripts/farming/hunt.ts", ""),
            ("scripts/node_modules/lib/index.js", ""),
            ("scripts/.git/hook.js", ""),
            ("community/daily.ts", ""),
            ("community/bosses/dage.ts", ""),
            (
                "packages/dailies/package.json",
                r#"{"name":"@lucent/dailies","version":"0.0.3","description":"Daily runs."}"#,
            ),
            ("packages/dailies/scripts/spin.ts", ""),
            ("packages/dailies/lib/wheel.ts", ""),
            ("packages/broken/package.json", "{"),
            ("packages/not-a-package/loose.ts", ""),
        ] {
            let path = temp.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let library = scan_all(&built_in, &[community.clone(), missing.clone()], &packages);
        let listed: Vec<(Option<usize>, Option<&str>, &str)> = library
            .entries
            .iter()
            .map(|entry| (entry.source, entry.folder.as_deref(), entry.name.as_ref()))
            .collect();
        assert_eq!(
            listed,
            [
                (None, None, "Alpha.js"),
                (None, None, "zeta.ts"),
                (None, Some("farming"), "daily.mts"),
                (None, Some("farming"), "hunt.ts"),
                (Some(0), None, "daily.ts"),
                (Some(0), Some("bosses"), "dage.ts"),
                (Some(3), None, "spin.ts"),
            ]
        );
        let sources: Vec<(&Path, &str, bool, usize)> = library
            .sources
            .iter()
            .map(|source| {
                (
                    source.root.as_path(),
                    source.name.as_ref(),
                    source.readable,
                    source.scripts,
                )
            })
            .collect();
        assert_eq!(
            sources,
            [
                (community.as_path(), "community", true, 2),
                (missing.as_path(), "unplugged", false, 0),
                (packages.join("broken").as_path(), "broken", false, 0),
                (
                    packages.join("dailies").as_path(),
                    "@lucent/dailies",
                    true,
                    1
                ),
            ]
        );
        assert_eq!(
            library.sources[3].kind,
            SourceKind::Package {
                version: Some("0.0.3".into()),
                description: Some("Daily runs.".into()),
            }
        );
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn search_and_the_folder_filter_narrow_the_list() {
        let entry = |name: &str, folder: Option<&str>, source: Option<usize>| Entry {
            path: PathBuf::from(name),
            name: name.to_owned().into(),
            folder: folder.map(|folder| folder.to_owned().into()),
            source,
        };
        let source = |name: &str| Source {
            root: PathBuf::from(name),
            name: name.to_owned().into(),
            kind: SourceKind::Folder,
            readable: true,
            scripts: 0,
        };
        let library = Library {
            entries: vec![
                entry("daily.ts", None, None),
                entry("hunt.ts", None, None),
                entry("daily.ts", Some("weekly"), Some(0)),
                entry("dage.ts", None, Some(0)),
                entry("spin.ts", None, Some(1)),
            ],
            sources: vec![source("community"), source("dailies")],
        };
        let names_in = |query: &str, scope: Scope| -> Vec<String> {
            library_rows(&library, query, scope)
                .iter()
                .map(|row| match row {
                    Row::Source(index) => format!("[{}]", library.sources[*index].name),
                    Row::Script(entry) => entry.name.to_string(),
                })
                .collect()
        };
        let names = |query: &str| names_in(query, Scope::All);
        assert_eq!(
            names(""),
            [
                "daily.ts",
                "hunt.ts",
                "[community]",
                "daily.ts",
                "dage.ts",
                "[dailies]",
                "spin.ts"
            ]
        );
        assert_eq!(names("daily"), ["daily.ts", "[community]", "daily.ts"]);
        assert_eq!(names("dailies"), ["[dailies]", "spin.ts"]);
        assert_eq!(names("weekly"), ["[community]", "daily.ts"]);
        assert!(names("zzz").is_empty());
        assert_eq!(names_in("", Scope::BuiltIn), ["daily.ts", "hunt.ts"]);
        assert_eq!(names_in("", Scope::Source(0)), ["daily.ts", "dage.ts"]);
        assert_eq!(names_in("daily", Scope::Source(0)), ["daily.ts"]);
        assert!(names_in("hunt", Scope::Source(1)).is_empty());
    }
}

fn ended(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Completed => "Finished",
        Outcome::Failed { .. } | Outcome::Killed(_) => "Failed",
        Outcome::Stopped => "Stopped",
        Outcome::Exited { .. } => "Exited",
    }
}

fn duration(took: std::time::Duration) -> String {
    let seconds = took.as_secs();
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        _ => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
    }
}
