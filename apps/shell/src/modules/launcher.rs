//! The launcher at the bottom of the screen. Typing searches the Start
//! menu's apps; a leading `>` lists commands (calculator, wallpaper,
//! colour variant, light/dark, session actions); arithmetic shows its
//! result live.

use std::path::PathBuf;

use norbanner_config::Config;
use norbanner_core::{calc, fuzzy, geom::Rect, sdf::Blob};
use norbanner_shell::{apps::App, session};

use super::categories::{self, Category};
use crate::{
    actions::Action,
    icons,
    render::paint::TextStyle,
    state,
    strings::text,
    ui::{Ctx, Ui},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    App(String),
    Calc(String),
    /// Puts this text in the search field (e.g. `>wallpaper `).
    Complete(String),
    Run(Action),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: Kind,
    pub title: String,
    pub subtitle: String,
    pub glyph: char,
}

#[derive(Default)]
pub struct LauncherState {
    pub query: String,
    pub selected: usize,
    pub first: usize,
    pub items: Vec<Item>,
    /// Nothing typed: the home view (greeting, shortcuts, frequent apps
    /// as a grid) instead of a result list.
    pub home: bool,
    /// What the home view shows below the pinned apps.
    pub view: View,
    /// The apps by category, the categories in display order, empty ones
    /// left out.
    pub categories: Vec<(Category, Vec<Item>)>,
    /// The home view's grid: the most launched apps, then alphabetical.
    pub pinned: Vec<Item>,
    /// The home view's app list: "Most used", then every app under its
    /// letter.
    pub list: Vec<Row>,
    /// The first row of `list` shown.
    pub list_first: usize,
    /// Ids of the apps pinned to Windows' taskbar: the grid starts with
    /// them.
    pub taskbar: Vec<String>,
    /// How much Windows has seen each app used (runs and minutes in
    /// focus), by app id.
    pub windows_usage: std::collections::HashMap<String, u32>,
    /// How many times each app was opened from the launcher.
    usage: std::collections::HashMap<String, u32>,
    usage_loaded: bool,
}

/// The home view: pinned apps over the categories, one category's apps,
/// or every app A–Z.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Categories,
    Category(usize),
    List,
}

/// A row of the home view's app list.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Header(String),
    App(Item),
}

/// Columns and rows of the home view's app grid.
pub const GRID_COLUMNS: usize = 6;
pub const GRID_ROWS: usize = 2;
/// Columns of a category's app grid.
pub const CATEGORY_COLUMNS: usize = 6;
/// Help files, manuals, uninstallers and websites Start lists beside the
/// apps they belong to.
fn is_helper(name: &str) -> bool {
    let name = norbanner_core::fuzzy::fold(name);
    [
        "help",
        "ayuda",
        "aide",
        "ajuda",
        "uninstall",
        "desinstal",
        "readme",
        "leame",
        "lisezmoi",
        "manual",
        "documentation",
        "documentacion",
        "website",
        "sitio web",
        "release notes",
        "notas de la version",
        "license",
        "licencia",
        "faq",
        "what's new",
        "novedades",
    ]
    .iter()
    .any(|word| name.contains(word))
}

/// Apps most people open often, to fill the grid before any record of use.
fn is_well_known(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    [
        "microsoft.windows.explorer",
        "immersivecontrolpanel",
        "microsoft.windowsstore",
        "windowscalculator",
        "microsoft.windows.photos",
        "windowsnotepad",
        "windowsterminal",
        "msedge",
        "chrome",
        "firefox",
        "spotify",
        "discord",
        "steam",
        "whatsapp",
        "telegram",
        "vscode",
        "visualstudiocode",
        "winword",
        "excel",
        "powerpnt",
        "outlook",
        "msteams",
        "paint",
        "screensketch",
        "windowscamera",
    ]
    .iter()
    .any(|token| id.contains(token))
}

/// Rows in the "Most used" part of the app list.
const MOST_USED: usize = 5;

fn usage_path() -> std::path::PathBuf {
    norbanner_config::data_dir().join("launches.json")
}

fn command(name: &str, desc: &str, glyph: char, kind: Kind) -> Item {
    Item {
        kind,
        title: name.to_string(),
        subtitle: desc.to_string(),
        glyph,
    }
}

impl LauncherState {
    pub fn clear(&mut self) {
        self.view = View::Categories;
        self.list_first = 0;
        self.query.clear();
        self.selected = 0;
        self.first = 0;
    }

    pub fn type_text(&mut self, text: &str) {
        self.query.push_str(text);
        self.selected = 0;
        self.first = 0;
    }

    pub fn backspace(&mut self, word: bool) {
        if word {
            let trimmed = self.query.trim_end().len();
            self.query.truncate(trimmed);
            let cut = self
                .query
                .rfind(|c: char| c.is_whitespace() || c == '>')
                .map_or(0, |i| i + 1);
            self.query.truncate(cut);
        } else {
            self.query.pop();
        }
        self.selected = 0;
        self.first = 0;
    }

    pub fn move_selection(&mut self, delta: i32, shown: usize) {
        if self.items.is_empty() {
            return;
        }
        let last = self.items.len() as i32 - 1;
        self.selected = (self.selected as i32 + delta).clamp(0, last) as usize;
        if self.selected < self.first {
            self.first = self.selected;
        } else if self.selected >= self.first + shown {
            self.first = self.selected + 1 - shown;
        }
    }

    pub fn scroll(&mut self, delta: i32, shown: usize) {
        let max_first = self.items.len().saturating_sub(shown) as i32;
        self.first = (self.first as i32 + delta).clamp(0, max_first) as usize;
        self.selected = self
            .selected
            .clamp(self.first, self.first + shown.saturating_sub(1));
    }

    /// Counts a launch of `id`, so it rises among the frequent apps.
    pub fn record(&mut self, id: &str) {
        self.load_usage();
        *self.usage.entry(id.to_string()).or_insert(0) += 1;
        if let Ok(text) = serde_json::to_string(&self.usage) {
            let path = usage_path();
            if let Some(folder) = path.parent() {
                let _ = std::fs::create_dir_all(folder);
            }
            let _ = std::fs::write(path, text);
        }
    }

    fn load_usage(&mut self) {
        if self.usage_loaded {
            return;
        }
        self.usage_loaded = true;
        if let Ok(text) = std::fs::read_to_string(usage_path()) {
            self.usage = serde_json::from_str(&text).unwrap_or_default();
        }
    }

    /// Builds the home view: the grid, "Most used" and the A–Z list.
    fn build_home(&mut self, apps: &[App]) {
        self.load_usage();
        let item = |app: &App| Item {
            kind: Kind::App(app.id.clone()),
            title: app.name.clone(),
            subtitle: String::new(),
            glyph: icons::APPS,
        };
        let mut alphabetical: Vec<&App> = apps.iter().collect();
        alphabetical.sort_by_key(|app| app.name.to_lowercase());
        // Launches from here weigh more than Windows' record of runs and
        // minutes in focus.
        let score = |app: &&App| {
            self.usage.get(&app.id).copied().unwrap_or(0) * 5
                + self.windows_usage.get(&app.id).copied().unwrap_or(0)
        };
        let pins: Vec<&App> = self
            .taskbar
            .iter()
            .filter_map(|id| apps.iter().find(|app| &app.id == id))
            .collect();
        let is_pin = |app: &&App| pins.iter().any(|p| p.id == app.id);
        let mut used: Vec<&App> = alphabetical
            .iter()
            .copied()
            .filter(|a| score(a) > 0 && !is_helper(&a.name))
            .collect();
        used.sort_by_key(|app| std::cmp::Reverse(score(app)));
        // "Most used" leaves out the pins, which lead the grid anyway.
        let most: Vec<&App> = used
            .iter()
            .copied()
            .filter(|a| !is_pin(a))
            .take(MOST_USED)
            .collect();
        // The grid: the taskbar's pins, then what is used most, then well
        // known apps, then the rest; never help files or uninstallers.
        let mut pinned: Vec<&App> = pins.clone();
        let well_known = alphabetical
            .iter()
            .copied()
            .filter(|a| is_well_known(&a.id));
        let rest = alphabetical.iter().copied().filter(|a| !is_helper(&a.name));
        for app in used
            .iter()
            .copied()
            .filter(|a| !most.iter().any(|m| m.id == a.id))
            .chain(well_known)
            .chain(rest)
        {
            if pinned.len() >= GRID_COLUMNS * GRID_ROWS {
                break;
            }
            if !pinned.iter().any(|p| p.id == app.id) {
                pinned.push(app);
            }
        }
        self.pinned = pinned.into_iter().map(item).collect();
        let mut list = Vec::new();
        if !most.is_empty() {
            list.push(Row::Header(text().most_used.to_string()));
            list.extend(most.into_iter().map(|a| Row::App(item(a))));
        }
        let mut letter = String::new();
        // Help files and uninstallers stay reachable by searching.
        for app in alphabetical.into_iter().filter(|a| !is_helper(&a.name)) {
            let first = app
                .name
                .chars()
                .next()
                .filter(|c| c.is_alphabetic())
                .map_or("#".to_string(), |c| c.to_uppercase().collect());
            if first != letter {
                letter = first.clone();
                list.push(Row::Header(first));
            }
            list.push(Row::App(item(app)));
        }
        self.list = list;
        self.list_first = self.list_first.min(self.list.len().saturating_sub(1));
        // Categories: most used first inside each.
        let mut ranked: Vec<&App> = apps.iter().filter(|a| !is_helper(&a.name)).collect();
        ranked.sort_by_key(|app| (std::cmp::Reverse(score(app)), app.name.to_lowercase()));
        self.categories = categories::ALL
            .iter()
            .map(|&category| {
                let items: Vec<Item> = ranked
                    .iter()
                    .copied()
                    .filter(|app| categories::classify(&app.name, &app.id) == category)
                    .map(item)
                    .collect();
                (category, items)
            })
            .filter(|(_, items)| !items.is_empty())
            .collect();
    }

    /// Scrolls the A–Z list, or a category's grid, by `delta` rows.
    pub fn scroll_list(&mut self, delta: i32) {
        let rows = match self.view {
            View::Categories => 0,
            View::List => self.list.len(),
            View::Category(i) => self
                .categories
                .get(i)
                .map_or(0, |(_, items)| items.len().div_ceil(CATEGORY_COLUMNS)),
        };
        let last = rows.saturating_sub(1) as i32;
        self.list_first = (self.list_first as i32 + delta).clamp(0, last) as usize;
    }

    /// Shows `view` in the home view, from its top.
    pub fn show(&mut self, view: View) {
        self.view = view;
        self.list_first = 0;
    }

    /// Moves the selection on the home grid.
    pub fn move_grid(&mut self, dx: i32, dy: i32) {
        if self.items.is_empty() {
            return;
        }
        let last = self.items.len() as i32 - 1;
        let next = self.selected as i32 + dx + dy * GRID_COLUMNS as i32;
        self.selected = next.clamp(0, last) as usize;
    }

    /// Rebuilds the rows for the current query.
    pub fn refresh(&mut self, apps: &[App], config: &Config) {
        let prefix = config.launcher.action_prefix.as_str();
        let query = self.query.clone();
        self.home = query.trim().is_empty();
        if self.home {
            self.build_home(apps);
            self.items = self.pinned.clone();
            self.selected = self.selected.min(self.items.len().saturating_sub(1));
            self.first = 0;
            return;
        }
        self.items =
            if let Some(rest) = query.strip_prefix(prefix) {
                commands(rest, config)
            } else {
                let mut items = Vec::new();
                if config.launcher.enable_calculator && calc::looks_like_math(&query) {
                    items.push(calc_item(&query));
                }
                let web = Item {
                    kind: Kind::Run(Action::WebSearch(query.trim().to_string())),
                    title: text().search_web.replace("{}", query.trim()),
                    subtitle: String::new(),
                    glyph: icons::TRAVEL_EXPLORE,
                };
                items.extend(fuzzy::rank(&query, apps, |a| &a.search).into_iter().map(
                    |(app, _)| Item {
                        kind: Kind::App(app.id.clone()),
                        title: app.name.clone(),
                        subtitle: String::new(),
                        glyph: icons::APPS,
                    },
                ));
                items.push(web);
                items
            };
        let shown = config.launcher.max_shown as usize;
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.first = self.first.min(self.items.len().saturating_sub(shown));
    }

    /// What running row `index` does: an action for the app, or `None`
    /// when it only changed the query (the caller refreshes).
    pub fn run(&mut self, index: usize, config: &Config) -> Option<Action> {
        let item = self.items.get(index)?.clone();
        match item.kind {
            Kind::App(id) => Some(Action::Launch(id)),
            Kind::Calc(result) => Some(Action::CopyText(result)),
            Kind::Complete(text) => {
                self.query = format!("{}{text}", config.launcher.action_prefix);
                self.selected = 0;
                self.first = 0;
                None
            }
            Kind::Run(action) => Some(action),
        }
    }
}

fn calc_item(expression: &str) -> Item {
    let result = calc::eval(expression).map(calc::format).unwrap_or_default();
    Item {
        kind: Kind::Calc(result.clone()),
        title: format!("{} = {result}", expression.trim()),
        subtitle: text().copy_result.to_string(),
        glyph: icons::CALCULATE,
    }
}

fn commands(rest: &str, config: &Config) -> Vec<Item> {
    let s = text();
    let (name, argument) = rest.split_once(' ').unwrap_or((rest, ""));
    match name {
        "calc" if rest.contains(' ') => {
            return if argument.trim().is_empty() {
                vec![command(
                    s.calculator,
                    s.action_calc_desc,
                    icons::CALCULATE,
                    Kind::Complete("calc ".into()),
                )]
            } else {
                vec![calc_item(argument)]
            };
        }
        "wallpaper" if rest.contains(' ') => return wallpapers(argument, config),
        "variant" if rest.contains(' ') => {
            return norbanner_theme::Variant::ALL
                .iter()
                .filter(|v| {
                    fuzzy::score(&fuzzy::fold(argument.trim()), &fuzzy::fold(v.id())).is_some()
                })
                .map(|v| Item {
                    kind: Kind::Run(Action::SetVariant(v.id().to_string())),
                    title: v.id().to_string(),
                    subtitle: if config.theme.variant == v.id() {
                        "✓".into()
                    } else {
                        String::new()
                    },
                    glyph: icons::PALETTE,
                })
                .collect();
        }
        _ => {}
    }
    let all = [
        command(
            s.action_calc,
            s.action_calc_desc,
            icons::CALCULATE,
            Kind::Complete("calc ".into()),
        ),
        command(
            s.action_wallpaper,
            s.action_wallpaper_desc,
            icons::WALLPAPER,
            Kind::Complete("wallpaper ".into()),
        ),
        command(
            s.action_variant,
            s.action_variant_desc,
            icons::PALETTE,
            Kind::Complete("variant ".into()),
        ),
        command(
            s.action_dark,
            s.action_dark_desc,
            icons::DARK_MODE,
            Kind::Run(Action::SetMode(true)),
        ),
        command(
            s.action_light,
            s.action_light_desc,
            icons::LIGHT_MODE,
            Kind::Run(Action::SetMode(false)),
        ),
        command(
            s.action_lock,
            s.action_lock_desc,
            icons::LOCK,
            Kind::Run(Action::Session(session::Action::Lock)),
        ),
        command(
            s.action_logout,
            s.action_logout_desc,
            icons::LOGOUT,
            Kind::Run(Action::Session(session::Action::Logout)),
        ),
        command(
            s.action_sleep,
            s.action_sleep_desc,
            icons::BEDTIME,
            Kind::Run(Action::Session(session::Action::Sleep)),
        ),
        command(
            s.action_shutdown,
            s.action_shutdown_desc,
            icons::POWER_SETTINGS_NEW,
            Kind::Run(Action::Session(session::Action::Shutdown)),
        ),
        command(
            s.action_reboot,
            s.action_reboot_desc,
            icons::RESTART_ALT,
            Kind::Run(Action::Session(session::Action::Reboot)),
        ),
        command(
            s.action_settings,
            s.action_settings_desc,
            icons::SETTINGS,
            Kind::Run(Action::OpenSettings),
        ),
        command(
            s.action_quit,
            s.action_quit_desc,
            icons::CLOSE,
            Kind::Run(Action::Quit),
        ),
    ];
    // Command ids in English work in every language ("wall", "shut").
    let ids = [
        "calc",
        "wallpaper",
        "variant",
        "dark",
        "light",
        "lock",
        "logout",
        "sleep",
        "shutdown",
        "reboot",
        "settings",
        "quit",
    ];
    let query = fuzzy::fold(rest.trim());
    let mut ranked: Vec<(i32, Item)> = all
        .into_iter()
        .zip(ids)
        .filter_map(|(item, id)| {
            let by_name = fuzzy::score(&query, &fuzzy::fold(&item.title));
            let by_id = fuzzy::score(&query, id);
            by_name.max(by_id).map(|score| (score, item))
        })
        .collect();
    if !query.is_empty() {
        ranked.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    }
    ranked.into_iter().map(|(_, item)| item).collect()
}

fn wallpapers(filter: &str, config: &Config) -> Vec<Item> {
    let Some(folder) = norbanner_shell::wallpaper::folder(&config.paths.wallpaper_dir) else {
        return vec![command(
            text().no_wallpapers,
            "",
            icons::IMAGE,
            Kind::Complete("wallpaper ".into()),
        )];
    };
    let query = fuzzy::fold(filter.trim());
    let files: Vec<PathBuf> = norbanner_shell::wallpaper::list(&folder);
    if files.is_empty() {
        return vec![command(
            text().no_wallpapers,
            "",
            icons::IMAGE,
            Kind::Complete("wallpaper ".into()),
        )];
    }
    files
        .into_iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().to_string();
            fuzzy::score(&query, &fuzzy::fold(&name))?;
            Some(Item {
                kind: Kind::Run(Action::SetWallpaper(path.clone())),
                title: name,
                subtitle: path.display().to_string(),
                glyph: icons::IMAGE,
            })
        })
        .collect()
}

// ---- The start menu -------------------------------------------------------
//
// Frosted glass, one column, compact:
//
// ┌──────────────────────────────────────────────────────┐
// │ [ 🔍 Search apps and commands                      ] │
// │ Pinned                                               │
// │  ▣    ▣    ▣    ▣    ▣    ▣                          │
// │  ▣    ▣    ▣    ▣    ▣    ▣                          │
// │ All                          [ Categories | A–Z ]    │
// │ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐                  │
// │ │ ▣ ▣  │ │ ▣ ▣  │ │ ▣ ▣  │ │ ▣ ▣  │  a category: its │
// │ │ ▣ +5 │ │ ▣ ▣  │ │ ▣    │ │ ▣ ▣  │  first apps and  │
// │ └Games─┘ └──────┘ └──────┘ └──────┘  how many more   │
// │ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐                  │
// │ └──────┘ └──────┘ └──────┘ └──────┘                  │
// ├──────────────────────────────────────────────────────┤
// │ (●) Name                    ✂  📋  ☺ │ 📁  ⚙  ⏻      │
// └──────────────────────────────────────────────────────┘
//
// A category opens in place as a grid; "A–Z" lists every app; typing
// shows the matches instead.

/// Sizes shared by `size`, `backdrop` and `draw`.
struct Metrics {
    d: f32,
    width: f32,
    height: f32,
    pad: f32,
    search: f32,
    title: f32,
    cell_h: f32,
    icon: f32,
    tile_h: f32,
    gap: f32,
    strip: f32,
    row: f32,
    header: f32,
}

fn metrics(ui: &Ctx) -> Metrics {
    let d = ui.t.bar_inner / 44.0;
    let pad = 22.0 * d;
    let search = 42.0 * d;
    let title = 34.0 * d;
    let cell_h = 84.0 * d;
    let tile_h = 118.0 * d;
    let gap = 10.0 * d;
    let strip = 60.0 * d;
    let width = 640.0 * d;
    let height = pad
        + search
        + 8.0 * d
        + title
        + cell_h * GRID_ROWS as f32
        + 6.0 * d
        + title
        + tile_h * 2.0
        + gap
        + pad
        + strip;
    Metrics {
        d,
        width,
        height,
        pad,
        search,
        title,
        cell_h,
        icon: 36.0 * d,
        tile_h,
        gap,
        strip,
        row: 44.0 * d,
        header: 28.0 * d,
    }
}

/// The same size whatever is shown, so the drawer never jumps.
pub fn size(ui: &Ctx, _launcher: &LauncherState) -> (f32, f32) {
    let m = metrics(ui);
    (m.width, m.height)
}

/// Over the glass (drawn by the surface shader), which keeps one tone
/// throughout: a hairline above the bottom strip.
pub fn backdrop(ui: &mut Ui, blob: Blob, content: Rect) {
    let pal = *ui.pal;
    let m = metrics(&ui.ctx());
    let strip_y = content.bottom() - m.strip;
    if strip_y > blob.rect.y {
        ui.p.rect(
            Rect::new(blob.rect.x, strip_y, blob.rect.w, 1.0),
            0.0,
            pal.outline_variant.with_alpha(0.5),
        );
    }
}

pub fn draw(ui: &mut Ui, rect: Rect, launcher: &LauncherState) {
    let ctx = ui.ctx();
    let m = metrics(&ctx);
    let inner = Rect::new(
        rect.x + m.pad,
        rect.y + m.pad,
        rect.w - m.pad * 2.0,
        rect.h - m.pad - m.strip,
    );
    let search = Rect::new(inner.x, inner.y, inner.w, m.search);
    search_field(ui, search, launcher);
    let body = Rect::new(
        inner.x,
        search.bottom() + 8.0 * m.d,
        inner.w,
        inner.bottom() - search.bottom() - 8.0 * m.d - m.pad,
    );
    if !launcher.home {
        results(ui, body, &m, launcher);
    } else {
        match launcher.view {
            View::Categories => {
                let pinned_h = m.title + m.cell_h * GRID_ROWS as f32;
                pinned(
                    ui,
                    Rect::new(body.x, body.y, body.w, pinned_h),
                    &m,
                    launcher,
                );
                let rest = Rect::new(
                    body.x,
                    body.y + pinned_h + 6.0 * m.d,
                    body.w,
                    body.h - pinned_h - 6.0 * m.d,
                );
                category_tiles(ui, rest, &m, launcher);
            }
            View::Category(index) => category_view(ui, body, &m, launcher, index),
            View::List => list_view(ui, body, &m, launcher),
        }
    }
    strip(
        ui,
        Rect::new(
            rect.x + m.pad,
            rect.bottom() - m.strip,
            rect.w - m.pad * 2.0,
            m.strip,
        ),
        &m,
    );
}

fn title(ui: &mut Ui, rect: Rect, text: &str) {
    let t = *ui.t;
    let pal = *ui.pal;
    ui.p.text(
        text,
        rect,
        TextStyle::new(t.font.smaller).weight(700.0),
        pal.on_surface,
    );
}

/// A title (with an icon, if any) after a "‹" button that goes back to
/// the categories.
fn back_title(ui: &mut Ui, rect: Rect, m: &Metrics, text: &str, glyph: Option<char>) {
    let t = *ui.t;
    let pal = *ui.pal;
    let side = rect.h;
    let back = Rect::new(rect.x - 6.0 * m.d, rect.y, side, side);
    ui.button(
        back,
        t.rounding.small,
        pal.on_surface,
        Action::LauncherView(View::Categories),
    );
    ui.p.icon(
        icons::CHEVRON_LEFT,
        back.center().0,
        back.center().1,
        side * 0.7,
        0.0,
        pal.on_surface,
    );
    let mut x = back.right();
    if let Some(glyph) = glyph {
        ui.p.icon(
            glyph,
            x + side * 0.4,
            rect.y + rect.h / 2.0,
            side * 0.62,
            1.0,
            pal.primary,
        );
        x += side * 0.9;
    }
    title(ui, Rect::new(x, rect.y, rect.right() - x, rect.h), text);
}

/// An app's icon (or the item's glyph) centred in `rect`.
fn item_icon(ui: &mut Ui, item: &Item, rect: Rect) {
    let pal = *ui.pal;
    if let Kind::App(id) = &item.kind
        && let Some(image) = ui.state.icons.get(id)
    {
        ui.p.image_fit(state::image_key(id), &image, rect);
        return;
    }
    let (cx, cy) = rect.center();
    ui.p.icon(item.glyph, cx, cy, rect.w * 0.8, 1.0, pal.primary);
}

/// What clicking an item does.
fn item_action(item: &Item, index: usize) -> Action {
    match &item.kind {
        Kind::App(id) => Action::Launch(id.clone()),
        _ => Action::LauncherRun(index),
    }
}

/// An app as a large icon over its name (up to two lines).
fn app_cell(ui: &mut Ui, cell: Rect, m: &Metrics, item: &Item, action: Action, selected: bool) {
    let t = *ui.t;
    let pal = *ui.pal;
    let hovered = ui.hovered(cell);
    if selected {
        ui.p.rect(cell, t.rounding.small, pal.primary.with_alpha(0.16));
    } else if hovered {
        ui.p.rect(cell, t.rounding.small, pal.on_surface.with_alpha(0.07));
    }
    ui.click(cell, action);
    let icon = Rect::new(
        cell.x + (cell.w - m.icon) / 2.0,
        cell.y + 8.0 * m.d,
        m.icon,
        m.icon,
    );
    item_icon(ui, item, icon);
    ui.p.text(
        &item.title,
        Rect::new(
            cell.x + 2.0,
            icon.bottom() + 5.0 * m.d,
            cell.w - 4.0,
            t.font.small * 2.6,
        ),
        TextStyle::new(t.font.small).center().wrap(),
        pal.on_surface,
    );
}

/// The pinned apps, two rows.
fn pinned(ui: &mut Ui, area: Rect, m: &Metrics, launcher: &LauncherState) {
    let s = text();
    title(ui, Rect::new(area.x, area.y, area.w, m.title), s.pinned);
    let top = area.y + m.title;
    let cell_w = area.w / GRID_COLUMNS as f32;
    for (index, item) in launcher.pinned.iter().enumerate() {
        let cell = Rect::new(
            area.x + (index % GRID_COLUMNS) as f32 * cell_w,
            top + (index / GRID_COLUMNS) as f32 * m.cell_h,
            cell_w,
            m.cell_h,
        )
        .inset(2.0 * m.d);
        let selected = index == launcher.selected;
        app_cell(ui, cell, m, item, item_action(item, index), selected);
    }
}

/// "All" with the view switch on the right, and the category tiles.
fn category_tiles(ui: &mut Ui, area: Rect, m: &Metrics, launcher: &LauncherState) {
    let t = *ui.t;
    let pal = *ui.pal;
    let s = text();
    let head = Rect::new(area.x, area.y, area.w, m.title);
    title(ui, head, s.everything);
    // The switch: a pill with the A–Z list.
    let style = TextStyle::new(t.font.small).weight(600.0);
    let (w, _) = ui.p.measure(s.view_list, style);
    let pill_w = w + t.icon.small + 22.0 * m.d;
    let pill = Rect::new(
        head.right() - pill_w,
        head.y + head.h * 0.14,
        pill_w,
        head.h * 0.72,
    );
    ui.p.rect(
        pill,
        t.rounding.small,
        pal.surface_container_highest.with_alpha(0.6),
    );
    ui.button(
        pill,
        t.rounding.small,
        pal.on_surface,
        Action::LauncherView(View::List),
    );
    ui.p.icon(
        icons::SORT_BY_ALPHA,
        pill.x + 8.0 * m.d + t.icon.small / 2.0,
        pill.center().1,
        t.icon.small * 0.9,
        0.0,
        pal.primary,
    );
    ui.p.text(
        s.view_list,
        Rect::new(pill.x + 12.0 * m.d + t.icon.small, pill.y, w + 4.0, pill.h),
        style,
        pal.on_surface,
    );

    let top = head.bottom();
    let columns = 4;
    let tile_w = (area.w - m.gap * (columns as f32 - 1.0)) / columns as f32;
    for (index, (category, items)) in launcher.categories.iter().enumerate().take(8) {
        let tile = Rect::new(
            area.x + (index % columns) as f32 * (tile_w + m.gap),
            top + (index / columns) as f32 * (m.tile_h + m.gap),
            tile_w,
            m.tile_h,
        );
        let hovered = ui.hovered(tile);
        ui.p.rect(
            tile,
            t.rounding.medium,
            pal.surface_container_highest
                .with_alpha(if hovered { 0.75 } else { 0.5 }),
        );
        ui.click(tile, Action::LauncherView(View::Category(index)));
        // Up to four app icons, two by two; the fourth spot counts the
        // rest when there are more.
        let label_h = t.font.small * 1.9;
        let grid = Rect::new(
            tile.x + tile.w * 0.18,
            tile.y + 10.0 * m.d,
            tile.w * 0.64,
            tile.h - label_h - 14.0 * m.d,
        );
        let side = (grid.w / 2.0).min(grid.h / 2.0);
        let icon = side * 0.62;
        let more = items.len() > 4;
        for (k, item) in items.iter().take(if more { 3 } else { 4 }).enumerate() {
            let cx = grid.x + grid.w / 4.0 + (k % 2) as f32 * grid.w / 2.0;
            let cy = grid.y + side / 2.0 + (k / 2) as f32 * side;
            item_icon(
                ui,
                item,
                Rect::new(cx - icon / 2.0, cy - icon / 2.0, icon, icon),
            );
        }
        if more {
            let cx = grid.x + grid.w * 0.75;
            let cy = grid.y + side * 1.5;
            // Wide enough for "+12".
            let w = icon * 1.3;
            let r = Rect::new(cx - w / 2.0, cy - icon / 2.0, w, icon);
            ui.p.rect(r, t.rounding.small, pal.primary.with_alpha(0.18));
            ui.p.text(
                &format!("+{}", items.len() - 3),
                r,
                TextStyle::new(t.font.small * 0.9).weight(700.0).center(),
                pal.primary,
            );
        }
        ui.p.text(
            category.label(),
            Rect::new(
                tile.x + 6.0,
                tile.bottom() - label_h - 4.0 * m.d,
                tile.w - 12.0,
                label_h,
            ),
            TextStyle::new(t.font.small).weight(600.0).center(),
            pal.on_surface,
        );
    }
}

/// One category's apps as a scrolling grid.
fn category_view(ui: &mut Ui, area: Rect, m: &Metrics, launcher: &LauncherState, index: usize) {
    let t = *ui.t;
    let pal = *ui.pal;
    let s = text();
    let Some((category, items)) = launcher.categories.get(index) else {
        return;
    };
    let head = Rect::new(area.x, area.y, area.w, m.title);
    back_title(ui, head, m, category.label(), Some(category.glyph()));
    let count = s.apps_count.replace("{}", &items.len().to_string());
    ui.p.text(
        &count,
        Rect::new(head.right() - 120.0 * m.d, head.y, 120.0 * m.d, head.h),
        TextStyle::new(t.font.small),
        pal.on_surface_variant,
    );
    let grid = Rect::new(area.x, head.bottom(), area.w, area.bottom() - head.bottom());
    ui.scroll(grid, Action::LauncherListScroll(0));
    let cell_w = grid.w / CATEGORY_COLUMNS as f32;
    let rows = ((grid.h / m.cell_h).floor() as usize).max(1);
    let first = launcher.list_first * CATEGORY_COLUMNS;
    for (k, item) in items
        .iter()
        .enumerate()
        .skip(first)
        .take(rows * CATEGORY_COLUMNS)
    {
        let slot = k - first;
        let cell = Rect::new(
            grid.x + (slot % CATEGORY_COLUMNS) as f32 * cell_w,
            grid.y + (slot / CATEGORY_COLUMNS) as f32 * m.cell_h,
            cell_w,
            m.cell_h,
        )
        .inset(2.0 * m.d);
        app_cell(ui, cell, m, item, item_action(item, 0), false);
    }
}

/// Every app A–Z ("Most used" first), in two columns of rows.
fn list_view(ui: &mut Ui, area: Rect, m: &Metrics, launcher: &LauncherState) {
    let t = *ui.t;
    let pal = *ui.pal;
    let s = text();
    let head = Rect::new(area.x, area.y, area.w, m.title);
    back_title(ui, head, m, s.all_apps, None);
    let list = Rect::new(area.x, head.bottom(), area.w, area.bottom() - head.bottom());
    ui.scroll(list, Action::LauncherListScroll(0));
    ui.p.clip(list);
    let col_w = (list.w - m.gap) / 2.0;
    let mut column = 0;
    let mut y = list.y;
    for row in launcher.list.iter().skip(launcher.list_first) {
        let h = match row {
            Row::Header(_) => m.header,
            Row::App(_) => m.row,
        };
        if y + h > list.bottom() + 0.5 {
            if column == 1 {
                break;
            }
            column = 1;
            y = list.y;
        }
        let x = list.x + column as f32 * (col_w + m.gap);
        match row {
            Row::Header(label) => ui.p.text(
                label,
                Rect::new(x + 4.0 * m.d, y, col_w, h),
                TextStyle::new(t.font.smaller).weight(700.0),
                pal.primary,
            ),
            Row::App(item) => app_row(
                ui,
                Rect::new(x, y, col_w, h).inset_xy(0.0, 1.0),
                m,
                item,
                item_action(item, 0),
                false,
            ),
        }
        y += h;
    }
    ui.p.unclip();
}

fn search_field(ui: &mut Ui, search: Rect, launcher: &LauncherState) {
    let t = *ui.t;
    let pal = *ui.pal;
    let a = pal.primary;
    let s = text();
    ui.p.rect(
        search,
        search.h / 2.0,
        pal.surface_container_highest.with_alpha(0.8),
    );
    let icon_x = search.x + t.padding.large + t.icon.small / 2.0;
    let (_, cy) = search.center();
    ui.p.icon(icons::SEARCH, icon_x, cy, t.icon.small, 0.0, a);
    let text_x = icon_x + t.icon.small / 2.0 + t.spacing.medium;
    let text_rect = Rect::new(
        text_x,
        search.y,
        search.right() - t.padding.large - text_x,
        search.h,
    );
    let style = TextStyle::new(t.font.smaller);
    let caret = |x: f32| Rect::new(x, search.y + search.h * 0.28, 1.5, search.h * 0.44);
    if launcher.query.is_empty() {
        ui.p.rect(caret(text_rect.x), 0.75, a);
        ui.p.text(
            s.launcher_placeholder,
            text_rect.offset(t.spacing.small, 0.0),
            style,
            pal.on_surface_variant,
        );
    } else {
        ui.p.text(&launcher.query, text_rect, style, pal.on_surface);
        let (w, _) = ui.p.measure(&launcher.query, style);
        ui.p.rect(
            caret((text_rect.x + w + 1.0).min(text_rect.right())),
            0.75,
            a,
        );
    }
}

/// An app (or result) row: icon and name, highlighted when `selected`.
fn app_row(ui: &mut Ui, r: Rect, m: &Metrics, item: &Item, action: Action, selected: bool) {
    let t = *ui.t;
    let pal = *ui.pal;
    let hovered = ui.hovered(r);
    if selected {
        ui.p.rect(r, t.rounding.small, pal.primary.with_alpha(0.16));
    } else if hovered {
        ui.p.rect(r, t.rounding.small, pal.on_surface.with_alpha(0.07));
    }
    ui.click(r, action);
    let side = 28.0 * m.d;
    let icon = Rect::new(r.x + 8.0 * m.d, r.y + (r.h - side) / 2.0, side, side);
    item_icon(ui, item, icon);
    let tx = icon.right() + 12.0 * m.d;
    let tw = r.right() - tx - 6.0 * m.d;
    let style = TextStyle::new(t.font.smaller).weight(if selected { 600.0 } else { 500.0 });
    if item.subtitle.is_empty() {
        ui.p.text(
            &item.title,
            Rect::new(tx, r.y, tw, r.h),
            style,
            pal.on_surface,
        );
    } else {
        let half = r.h / 2.0;
        ui.p.text(
            &item.title,
            Rect::new(tx, r.y + half * 0.15, tw, half),
            style,
            pal.on_surface,
        );
        ui.p.text(
            &item.subtitle,
            Rect::new(tx, r.y + half * 0.95, tw, half * 0.8),
            TextStyle::new(t.font.small),
            pal.on_surface_variant,
        );
    }
}

/// Something typed: the matches, the selected one highlighted, in two
/// columns.
fn results(ui: &mut Ui, area: Rect, m: &Metrics, launcher: &LauncherState) {
    let t = *ui.t;
    let pal = *ui.pal;
    let s = text();
    if launcher.items.is_empty() {
        ui.p.text(
            s.no_results,
            Rect::new(area.x, area.y, area.w, m.row),
            TextStyle::new(t.font.smaller).center(),
            pal.on_surface_variant,
        );
        return;
    }
    ui.scroll(area, Action::LauncherSelect(usize::MAX));
    let fit = ((area.h / m.row).floor() as usize).max(1);
    for (row, index) in (launcher.first..launcher.items.len()).take(fit).enumerate() {
        let item = &launcher.items[index];
        let r = Rect::new(area.x, area.y + row as f32 * m.row, area.w, m.row).inset_xy(0.0, 1.0);
        app_row(
            ui,
            r,
            m,
            item,
            Action::LauncherRun(index),
            index == launcher.selected,
        );
    }
}

/// The account on the left; tools, files, settings and power on the right.
fn strip(ui: &mut Ui, rect: Rect, m: &Metrics) {
    let t = *ui.t;
    let pal = *ui.pal;
    let side = 34.0 * m.d;
    let cy = rect.y + rect.h / 2.0;
    let avatar = Rect::new(rect.x, cy - side / 2.0, side, side);
    if let Some(image) = ui.state.avatar.clone() {
        ui.p.image(state::image_key("avatar"), &image, avatar, side / 2.0);
    } else {
        ui.p.rect(avatar, side / 2.0, pal.primary_container);
        ui.p.icon(
            icons::ACCOUNT_CIRCLE,
            avatar.center().0,
            cy,
            side * 0.8,
            1.0,
            pal.on_primary_container,
        );
    }
    let name = ui.state.user_name.clone();
    let buttons = [
        (icons::SCREENSHOT_REGION, Action::Screenshot),
        (icons::CONTENT_PASTE, Action::ClipboardHistory),
        (icons::MOOD, Action::EmojiPanel),
        (
            icons::FOLDER,
            Action::OpenPath(PathBuf::from("explorer.exe")),
        ),
        (icons::SETTINGS, Action::OpenSettings),
        (icons::POWER_SETTINGS_NEW, Action::ToggleSession),
    ];
    let gap = 4.0 * m.d;
    let total = buttons.len() as f32 * (side + gap) - gap + 10.0 * m.d;
    ui.p.text(
        &name,
        Rect::new(
            avatar.right() + 10.0 * m.d,
            rect.y,
            rect.w - side - 20.0 * m.d - total,
            rect.h,
        ),
        TextStyle::new(t.font.smaller).weight(600.0),
        pal.on_surface,
    );
    let mut x = rect.right() - total;
    for (k, (glyph, action)) in buttons.into_iter().enumerate() {
        // A thin divider between the tools and the system buttons.
        if k == 3 {
            ui.p.rect(
                Rect::new(x + 4.0 * m.d, cy - side * 0.3, 1.0, side * 0.6),
                0.0,
                pal.outline_variant,
            );
            x += 10.0 * m.d;
        }
        let r = Rect::new(x, cy - side / 2.0, side, side);
        let hovered = ui.hovered(r);
        if hovered {
            ui.p.rect(r, t.rounding.small, pal.on_surface.with_alpha(0.08));
        }
        ui.click(r, action);
        ui.p.icon(
            glyph,
            r.center().0,
            cy,
            side * 0.56,
            0.0,
            if hovered { pal.primary } else { pal.on_surface },
        );
        x += side + gap;
    }
}
