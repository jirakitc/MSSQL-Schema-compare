use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use egui_extras::{Column, TableBuilder};
use schema_compare::{
    comparison::compare_objects as compare,
    database_browser::DatabaseBrowser,
    diff_view::{aligned, Row as DiffRow},
    engine::{plan, Difference, Status},
    model::Snapshot,
    project::{
        active_redgate_project, import_redgate, redgate_project_directory, Endpoint, Project,
    },
    recent_connections::{history_path, RecentConnections},
};
#[path = "ui_about.rs"]
mod about;
#[path = "ui_editor.rs"]
mod editor;
#[path = "ui_recent.rs"]
mod recent;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

const BLUE: Color32 = Color32::from_rgb(46, 96, 155);
const TEXT: Color32 = Color32::from_rgb(38, 38, 38);
const MUTED: Color32 = Color32::from_rgb(104, 104, 104);
const BORDER: Color32 = Color32::from_rgb(207, 207, 207);
const DARK: Color32 = Color32::from_rgb(58, 58, 58);
const YELLOW: Color32 = Color32::from_rgb(255, 242, 193);
const GREEN: Color32 = Color32::from_rgb(232, 245, 227);
const RED: Color32 = Color32::from_rgb(255, 231, 231);

fn matches_filter(
    d: &Difference,
    statuses: &[bool; 3],
    query: &str,
    hidden_kinds: &BTreeSet<String>,
    show_identical: bool,
) -> bool {
    let status = match d.status {
        Status::Different => statuses[0],
        Status::SourceOnly => statuses[1],
        Status::TargetOnly => statuses[2],
        Status::Equal => show_identical,
    };
    status
        && !hidden_kinds.contains(&d.object().kind.to_string())
        && (query.is_empty() || d.key.to_lowercase().contains(query))
}

fn select_matching(
    diffs: &mut [Difference],
    statuses: &[bool; 3],
    query: &str,
    hidden_kinds: &BTreeSet<String>,
    show_identical: bool,
) {
    for d in diffs {
        d.selected = d.status != Status::Equal
            && matches_filter(d, statuses, query, hidden_kinds, show_identical);
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    fn rows() -> Vec<Difference> {
        [
            Status::Different,
            Status::SourceOnly,
            Status::TargetOnly,
            Status::Equal,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, status)| {
            let object = schema_compare::model::Object {
                schema: "dbo".into(),
                name: format!("Item{i}"),
                kind: if i == 1 {
                    schema_compare::model::Kind::Procedure
                } else {
                    schema_compare::model::Kind::Table
                },
                definition: "SELECT 1".into(),
                parent: None,
                dependencies: vec![],
                columns: vec![],
                ansi_nulls: None,
                quoted_identifier: None,
            };
            Difference {
                key: object.key(),
                source: Some(object),
                target: None,
                status,
                selected: true,
                components: vec![],
            }
        })
        .collect()
    }
    #[test]
    fn multi_status_filter_selects_only_matching_rows_and_clears_hidden_selections() {
        let mut diffs = rows();
        select_matching(
            &mut diffs,
            &[true, false, true],
            "",
            &BTreeSet::new(),
            false,
        );
        assert_eq!(
            diffs.iter().map(|d| d.selected).collect::<Vec<_>>(),
            [true, false, true, false]
        );
        assert_eq!(
            diffs
                .iter()
                .filter(|d| matches_filter(d, &[true, false, true], "", &BTreeSet::new(), false))
                .count(),
            2
        );
        select_matching(
            &mut diffs,
            &[false, true, false],
            "",
            &BTreeSet::new(),
            false,
        );
        assert_eq!(
            diffs.iter().map(|d| d.selected).collect::<Vec<_>>(),
            [false, true, false, false]
        );
    }
    #[test]
    fn search_and_object_type_filters_use_the_same_rules_as_selection() {
        let mut diffs = rows();
        select_matching(&mut diffs, &[true; 3], "item1", &BTreeSet::new(), false);
        assert_eq!(
            diffs.iter().map(|d| d.selected).collect::<Vec<_>>(),
            [false, true, false, false]
        );
        let hidden = BTreeSet::from(["Procedure".into()]);
        select_matching(&mut diffs, &[true; 3], "item1", &hidden, false);
        assert!(diffs.iter().all(|d| !d.selected));
    }
    #[test]
    fn no_active_status_clears_changes_and_identical_is_never_deployed() {
        let mut diffs = rows();
        select_matching(&mut diffs, &[false; 3], "", &BTreeSet::new(), true);
        assert!(diffs.iter().all(|d| !d.selected));
        assert!(matches_filter(
            &diffs[3],
            &[false; 3],
            "",
            &BTreeSet::new(),
            true
        ));
    }
}

fn object_name_label(ui: &mut egui::Ui, name: &str) -> egui::Response {
    ui.add(
        egui::Label::new(name)
            .truncate()
            .selectable(false)
            .sense(egui::Sense::click()),
    )
    .on_hover_cursor(egui::CursorIcon::Default)
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> (egui::Rect, bool, egui::CursorIcon) {
        let mut rect = egui::Rect::NOTHING;
        let mut clicked = false;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(600.0, 300.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    // Reproduce the global default which caused selectable labels to consume clicks.
                    ui.style_mut().interaction.selectable_labels = true;
                    let response = object_name_label(ui, "Customers");
                    rect = response.rect;
                    clicked = response.clicked();
                });
            },
        );
        (rect, clicked, output.platform_output.cursor_icon)
    }
    #[test]
    fn table_name_uses_arrow_cursor_and_accepts_a_normal_click() {
        let ctx = egui::Context::default();
        let (rect, _, _) = frame(&ctx, vec![]);
        let at = rect.center();
        let (_, clicked, cursor) = frame(&ctx, vec![egui::Event::PointerMoved(at)]);
        assert_eq!(cursor, egui::CursorIcon::Default);
        assert!(!clicked);
        frame(
            &ctx,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        let (_, clicked, cursor) = frame(
            &ctx,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert!(
            clicked,
            "Clicking the table name was consumed by text selection"
        );
        assert_eq!(cursor, egui::CursorIcon::Default);
    }
}

enum Event {
    Progress(String),
    Loaded(Result<(Snapshot, Snapshot), String>),
    Imported(Result<Project, String>),
    Deployed(schema_compare::deployment::DeploymentOutcome),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Grouping {
    Difference,
    Object,
    None,
}
impl Grouping {
    fn label(self) -> &'static str {
        match self {
            Self::Difference => "Type of difference",
            Self::Object => "Type of object",
            Self::None => "No groups",
        }
    }
}
enum ResultRow {
    Group { key: String, indices: Vec<usize> },
    Object(usize),
}

pub struct App {
    brand_icon: egui::TextureHandle,
    project: Project,
    source: Option<Snapshot>,
    target: Option<Snapshot>,
    diffs: Vec<Difference>,
    pending: Option<Receiver<Event>>,
    pending_started: Option<std::time::Instant>,
    deploy_open: bool,
    deployment_notice: Option<(bool, String)>,
    comparison_stale: bool,
    source_status: String,
    target_status: String,
    message: String,
    error: bool,
    query: String,
    grouping: Grouping,
    collapsed: BTreeSet<String>,
    hidden_kinds: BTreeSet<String>,
    show_identical: bool,
    status_filters: [bool; 3],
    filter_selection: bool,
    selected: Option<usize>,
    diff_rows: Vec<DiffRow>,
    diff_horizontal: f32,
    diff_jump: Option<f32>,
    diff_cursor: Option<usize>,
    tab: usize,
    script: String,
    blockers: Vec<String>,
    warnings: Vec<String>,
    actions: usize,
    edit_open: bool,
    edit_tab: usize,
    about_open: bool,
    about_panel: about::AboutPanel,
    filters_open: bool,
    project_path: Option<PathBuf>,
    table_height: f32,
    split_drag: bool,
    compare_on_start: bool,
    loaded_connections: Option<(String, String)>,
    database_browsers: [DatabaseBrowser; 2],
    recent_connections: RecentConnections,
    recent_path: PathBuf,
    recent_search: String,
    show_recent: bool,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Light);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.panel_fill = Color32::WHITE;
        style.visuals.window_fill = Color32::WHITE;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.selection.bg_fill = Color32::from_rgb(210, 230, 252);
        style.visuals.selection.stroke = Stroke::new(1.0_f32, BLUE);
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        // Match SQL Data Compare's default egui sizing and spacing.
        cc.egui_ctx.set_style(style);
        if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\tahoma.ttf") {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "windows_thai".into(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "windows_thai".into());
            cc.egui_ctx.set_fonts(fonts);
        }
        let icon =
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/schema-compare.png"))
                .expect("Embedded application icon must be valid");
        let brand_icon = cc.egui_ctx.load_texture(
            "schema_compare_brand",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
        let recent_path = history_path();
        let (recent_connections, recent_error) = match RecentConnections::load(&recent_path) {
            Ok(history) => (history, None),
            Err(error) => (
                RecentConnections::default(),
                Some(format!("Recent connections could not be loaded: {error:#}")),
            ),
        };
        let mut app = Self {
            brand_icon,
            project: Project::default(),
            source: None,
            target: None,
            diffs: vec![],
            pending: None,
            pending_started: None,
            deploy_open: false,
            deployment_notice: None,
            comparison_stale: false,
            source_status: String::new(),
            target_status: String::new(),
            message: "Choose a project or recent connections to compare SQL Server schemas.".into(),
            error: false,
            query: String::new(),
            grouping: Grouping::Object,
            collapsed: BTreeSet::from(["3:Identical objects".into()]),
            hidden_kinds: BTreeSet::new(),
            show_identical: false,
            status_filters: [true; 3],
            filter_selection: false,
            selected: None,
            diff_rows: vec![],
            diff_horizontal: 0.0,
            diff_jump: None,
            diff_cursor: None,
            tab: 0,
            script: String::new(),
            blockers: vec![],
            warnings: vec![],
            actions: 0,
            edit_open: false,
            edit_tab: 0,
            about_open: false,
            about_panel: Default::default(),
            filters_open: false,
            project_path: None,
            table_height: 320.0,
            split_drag: false,
            compare_on_start: false,
            loaded_connections: None,
            database_browsers: Default::default(),
            recent_connections,
            recent_path,
            recent_search: String::new(),
            show_recent: false,
        };
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if let Some(i) = args
            .iter()
            .position(|a| a == "--project" || a == "--redgate-project")
        {
            if let Some(path) = args.get(i + 1) {
                let path = PathBuf::from(path);
                match Project::read(&path) {
                    Ok(p) => app.apply_project(p, Some(path)),
                    Err(e) => app.set_message(Err(e)),
                }
            }
        }
        if app.project_path.is_none() {
            if let Ok(path) = active_redgate_project() {
                if let Ok(project) = import_redgate(&path, true) {
                    app.apply_project(project, Some(path));
                }
            }
        }
        app.compare_on_start = args.iter().any(|a| a == "--compare");
        if let Some(error) = recent_error {
            app.message = error;
            app.error = true;
        }
        app
    }
    fn set_message(&mut self, r: anyhow::Result<String>) {
        match r {
            Ok(s) => {
                self.message = s;
                self.error = false;
            }
            Err(e) => {
                self.message = format!("{e:#}");
                self.error = true;
            }
        }
    }
    fn clear_results(&mut self) {
        self.loaded_connections = None;
        self.source = None;
        self.target = None;
        self.diffs.clear();
        self.deploy_open = false;
        self.deployment_notice = None;
        self.comparison_stale = false;
        self.selected = None;
        self.diff_rows.clear();
        self.script.clear();
        self.blockers.clear();
        self.warnings.clear();
        self.actions = 0;
    }
    fn apply_project(&mut self, p: Project, path: Option<PathBuf>) {
        self.show_recent = false;
        self.database_browsers = Default::default();
        self.clear_results();
        self.project = p;
        self.project_path = path;
        self.edit_open = self.project_path.is_some()
            || self.project.source.snapshot
            || self.project.target.snapshot
            || !self.project.source.connection.server.trim().is_empty()
            || !self.project.target.connection.server.trim().is_empty();
        self.edit_tab = 0;
        self.error = false;
        self.message = if self.edit_open {
            "Project loaded. Source and target are ready to compare."
        } else {
            "Choose recent connections or Edit project to start a comparison."
        }
        .into();
    }
    fn rebuild(&mut self) {
        if let (Some(s), Some(t)) = (&self.source, &self.target) {
            let p = plan(s, t, &self.diffs, &self.project.options);
            self.script = p.sql;
            self.blockers = p.blockers;
            self.warnings = p.warnings;
            self.warnings.extend(self.project.import_notes.clone());
            self.actions = p.actions;
        }
    }
    fn select_filtered(&mut self) {
        self.filter_selection = true;
        let query = self.query.to_lowercase();
        select_matching(
            &mut self.diffs,
            &self.status_filters,
            &query,
            &self.hidden_kinds,
            self.show_identical,
        );
        if self.selected.is_none_or(|i| {
            self.diffs.get(i).is_none_or(|d| {
                !matches_filter(
                    d,
                    &self.status_filters,
                    &query,
                    &self.hidden_kinds,
                    self.show_identical,
                )
            })
        }) {
            if let Some(i) = self.diffs.iter().position(|d| {
                matches_filter(
                    d,
                    &self.status_filters,
                    &query,
                    &self.hidden_kinds,
                    self.show_identical,
                )
            }) {
                self.choose_object(i);
            } else {
                self.selected = None;
                self.diff_rows.clear();
            }
        }
        self.rebuild();
    }
    fn recompare(&mut self) {
        let selected_key = self
            .selected
            .and_then(|index| self.diffs.get(index))
            .map(|d| d.key.clone());
        let selection: BTreeMap<_, _> = self
            .diffs
            .iter()
            .map(|d| (d.key.clone(), d.selected))
            .collect();
        if let (Some(source), Some(target)) = (&self.source, &self.target) {
            self.diffs = compare(source, target, &self.project.options);
            for d in &mut self.diffs {
                if d.status != Status::Equal {
                    if let Some(value) = selection.get(&d.key) {
                        d.selected = *value;
                    }
                }
            }
        }
        if let Some(index) = selected_key
            .and_then(|key| self.diffs.iter().position(|d| d.key == key))
            .or_else(|| {
                self.diffs
                    .iter()
                    .position(|d| d.status == Status::Different)
            })
            .or_else(|| self.diffs.iter().position(|d| d.status != Status::Equal))
        {
            self.choose_object(index);
        } else {
            self.selected = None;
            self.diff_rows.clear();
        }
        self.rebuild();
    }
    fn choose_object(&mut self, index: usize) {
        self.selected = Some(index);
        let d = &self.diffs[index];
        self.diff_rows = aligned(
            d.source
                .as_ref()
                .map(|o| o.definition.as_str())
                .unwrap_or(""),
            d.target
                .as_ref()
                .map(|o| o.definition.as_str())
                .unwrap_or(""),
        );
        self.diff_horizontal = 0.0;
        self.diff_cursor = None;
        self.diff_jump = Some(0.0);
    }
    fn accept(&mut self, s: Snapshot, t: Snapshot) {
        self.show_recent = false;
        self.comparison_stale = false;
        self.tab = 0;
        self.loaded_connections = Some((
            endpoint_stamp(&self.project.source),
            endpoint_stamp(&self.project.target),
        ));
        self.diffs = compare(&s, &t, &self.project.options);
        self.message=format!("Comparison complete. {} source objects · {} target objects. Table components are included in their parent row.",self.diffs.iter().filter(|d|d.source.is_some()).count(),self.diffs.iter().filter(|d|d.target.is_some()).count());
        self.error = false;
        self.source = Some(s);
        self.target = Some(t);
        if self.filter_selection
            || self.status_filters != [true; 3]
            || !self.query.is_empty()
            || !self.hidden_kinds.is_empty()
        {
            self.select_filtered();
        }
        if let Some(i) = self.diffs.iter().position(|d| {
            d.status != Status::Equal
                && matches_filter(
                    d,
                    &self.status_filters,
                    &self.query.to_lowercase(),
                    &self.hidden_kinds,
                    self.show_identical,
                )
        }) {
            self.choose_object(i);
        } else {
            self.selected = None;
            self.diff_rows.clear();
        }
        self.rebuild();
        self.edit_open = false;
        if let Err(error) = self
            .recent_connections
            .remember(&self.project, &self.recent_path)
        {
            self.message.push_str(&format!(
                " Recent connections could not be saved: {error:#}"
            ));
        }
    }
    fn start(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let a = self.project.source.clone();
        let b = self.project.target.clone();
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.pending_started = Some(std::time::Instant::now());
        self.message = "Connecting to Source and Target concurrently...".into();
        self.source_status = "Connecting...".into();
        self.target_status = "Connecting...".into();
        self.deployment_notice = None;
        self.error = false;
        self.edit_open = false;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let progress = |side: &str, stage: &str| {
                    let _ = tx.send(Event::Progress(format!("{side}: {stage}")));
                    ctx.request_repaint();
                };
                let (s,t)=schema_compare::loading::load_pair(&a,&b,progress)?;
                progress("Compare", "Matching objects and checking columns, SQL definitions, constraints and indexes");
                Ok((s, t))
            })()
            .map_err(|e: anyhow::Error| format!("{e:#}"));
            let _ = tx.send(Event::Loaded(result));
            ctx.request_repaint();
        });
    }
    fn deploy_ready(&self) -> bool {
        self.pending.is_none()
            && !self.comparison_stale
            && !self.project.target.snapshot
            && self.source.is_some()
            && self.target.is_some()
            && self.blockers.is_empty()
            && self.actions > 0
    }
    fn start_deploy(&mut self, ctx: &egui::Context) {
        if !self.deploy_ready() {
            return;
        }
        let project = self.project.clone();
        let source = self.source.clone().unwrap();
        let target = self.target.clone().unwrap();
        let selection = self.diffs.clone();
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.pending_started = Some(std::time::Instant::now());
        self.message =
            "Preparing deployment and checking current Source / Target schemas...".into();
        self.error = false;
        self.source_status = "Waiting for preflight...".into();
        self.target_status = "Waiting for preflight...".into();
        self.deployment_notice = None;
        self.deploy_open = false;
        self.edit_open = false;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = schema_compare::deployment::deploy_and_refresh(
                &project,
                &source,
                &target,
                &selection,
                |stage| {
                    let _ = tx.send(Event::Progress(stage.into()));
                    ctx.request_repaint();
                },
            );
            let _ = tx.send(Event::Deployed(result));
            ctx.request_repaint();
        });
    }
    fn deploy_dialog(&mut self, ctx: &egui::Context) {
        if !self.deploy_open {
            return;
        }
        let mut open = true;
        let mut run = false;
        egui::Window::new("Deploy to Target").open(&mut open).collapsible(false).default_width(650.0).show(ctx,|ui|{
            ui.strong(format!("Target: {} / {}",self.project.target.connection.server,self.project.target.connection.database));
            ui.label(format!("{} selected objects · {} schema actions",self.diffs.iter().filter(|d|d.selected && d.status!=Status::Equal).count(),self.actions));
            ui.label("This will modify the Target database. Source and Target schemas are checked again before execution; changed schemas require a new Compare.");
            if self.project.options.allow_drops {ui.colored_label(Color32::from_rgb(178,37,37),"Destructive drops are enabled. Selected target-only objects will be removed.");}
            ui.separator();
            egui::ScrollArea::both().max_height(220.0).show(ui,|ui|{ui.label(RichText::new(&self.script).monospace());});
            ui.separator();
            run=ui.add_enabled(self.deploy_ready(),egui::Button::new(RichText::new("Deploy to Target").color(Color32::WHITE)).fill(BLUE)).clicked();
        });
        self.deploy_open = open;
        if run {
            self.start_deploy(ctx);
        }
    }
    fn deployment_message(&mut self, ctx: &egui::Context) {
        let Some((success, message)) = &self.deployment_notice else {
            return;
        };
        let mut open = true;
        let mut close = false;
        egui::Window::new(if *success {
            "Deployment Success"
        } else {
            "Deployment Fail"
        })
        .id(egui::Id::new("deployment_result"))
        .open(&mut open)
        .collapsible(false)
        .default_width(620.0)
        .max_width(850.0)
        .show(ctx, |ui| {
            ui.colored_label(
                if *success {
                    Color32::DARK_GREEN
                } else {
                    Color32::from_rgb(178, 37, 37)
                },
                if *success { "Success" } else { "Fail" },
            );
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    ui.label(message);
                });
            close = ui.button("Back to Compare").clicked();
        });
        if !open || close {
            self.deployment_notice = None;
            self.tab = 0;
        }
    }
    fn import(&mut self, path: Option<PathBuf>, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.pending_started = Some(std::time::Instant::now());
        self.message = "Reading Redgate project and its saved connection settings...".into();
        self.error = false;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let path = match path {
                    Some(p) => p,
                    None => active_redgate_project()?,
                };
                import_redgate(&path, true)
            })()
            .map_err(|e: anyhow::Error| format!("{e:#}"));
            let _ = tx.send(Event::Imported(result));
            ctx.request_repaint();
        });
    }
    fn import_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(redgate_project_directory())
            .add_filter("SQL Compare project", &["scp"])
            .pick_file()
        {
            self.import(Some(path), ctx);
        }
    }
    fn project_io(&mut self, save: bool) {
        if save {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Schema Compare project", &["json"])
                .set_file_name("comparison.project.json")
                .save_file()
            {
                let r = self.project.write(&path).map(|_| {
                    format!(
                        "Project saved to {}. Credentials stay in memory.",
                        path.display()
                    )
                });
                if r.is_ok() {
                    self.project_path = Some(path);
                }
                self.set_message(r);
            }
        } else if let Some(path) = rfd::FileDialog::new()
            .add_filter("Comparison project", &["json", "scp"])
            .pick_file()
        {
            match Project::read(&path) {
                Ok(p) => self.apply_project(p, Some(path)),
                Err(e) => self.set_message(Err(e)),
            }
        }
    }
    fn snapshot_export(&mut self, source: bool) {
        if let Some(s) = if source { &self.source } else { &self.target } {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Schema snapshot", &["json"])
                .set_file_name(if source { "source.json" } else { "target.json" })
                .save_file()
            {
                let r = s
                    .write(&path)
                    .map(|_| format!("Snapshot saved: {}", path.display()));
                self.set_message(r);
            }
        }
    }
    fn report_export(&mut self) {
        if let (Some(s), Some(t)) = (&self.source, &self.target) {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("comparison-report.json")
                .save_file()
            {
                let result = (|| {
                    std::fs::write(
                        &path,
                        serde_json::to_string_pretty(
                            &serde_json::json!({"source":s.label,"target":t.label,"differences":self.diffs,"warnings":self.warnings,"blockers":self.blockers}),
                        )?,
                    )?;
                    Ok(format!("Report saved: {}", path.display()))
                })();
                self.set_message(result);
            }
        }
    }
    fn script_export(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("SQL script", &["sql"])
            .set_file_name("deployment.sql")
            .save_file()
        {
            let r = schema_compare::model::write_script(&path, &self.script)
                .map(|_| format!("SQL script saved: {}", path.display()));
            self.set_message(r);
        }
    }
    fn swap(&mut self) {
        std::mem::swap(&mut self.project.source, &mut self.project.target);
        self.clear_results();
        self.message = "Direction changed. Compare again to read the new source and target.".into();
    }
    fn menus(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let menu_style = ui.style().clone();
        ui.visuals_mut().widgets.inactive.weak_bg_fill = DARK;
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
        ui.visuals_mut().widgets.hovered.weak_bg_fill = Color32::from_rgb(85, 85, 85);
        ui.horizontal(|ui| {
            for label in ["Project", "Compare", "Deployment", "Export", "Help"] {
                ui.menu_button(RichText::new(label).color(Color32::WHITE), |ui| {
                    ui.set_style(menu_style.clone());
                    match label {
                        "Project" => {
                            if ui.button("New project").clicked() {
                                self.apply_project(Project::default(), None);
                                ui.close_menu();
                            }
                            if ui.button("Open project...").clicked() {
                                self.project_io(false);
                                ui.close_menu();
                            }
                            if ui.button("Save project as...").clicked() {
                                self.project_io(true);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Edit project...").clicked() {
                                self.edit_open = true;
                                self.edit_tab = 0;
                                ui.close_menu();
                            }
                            if ui.button("Recent connections").clicked() {
                                self.show_recent = true;
                                self.edit_open = false;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Import Redgate project (.scp)...").clicked() {
                                self.import_dialog(ctx);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Exit").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        "Compare" => {
                            if ui.button("Compare now / Refresh       F5").clicked() {
                                self.start(ctx);
                                ui.close_menu();
                            }
                            if ui.button("Swap source and target").clicked() {
                                self.swap();
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(
                                    self.source.is_some(),
                                    egui::Button::new("Comparison results"),
                                )
                                .clicked()
                            {
                                self.show_recent = false;
                                self.edit_open = false;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Ignore options...").clicked() {
                                self.edit_open = true;
                                self.edit_tab = 1;
                                ui.close_menu();
                            }
                            if ui.button("Filter object types...").clicked() {
                                self.filters_open = true;
                                ui.close_menu();
                            }
                        }
                        "Deployment" => {
                            if ui
                                .add_enabled(
                                    self.source.is_some(),
                                    egui::Button::new("Review SQL..."),
                                )
                                .clicked()
                            {
                                self.tab = 1;
                                self.show_recent = false;
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(self.deploy_ready(), egui::Button::new("Deploy..."))
                                .clicked()
                            {
                                self.deploy_open = true;
                                ui.close_menu();
                            }
                        }
                        "Export" => {
                            if ui
                                .add_enabled(
                                    self.source.is_some(),
                                    egui::Button::new("Export comparison report..."),
                                )
                                .clicked()
                            {
                                self.report_export();
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(
                                    self.blockers.is_empty() && self.actions > 0,
                                    egui::Button::new("Save SQL script..."),
                                )
                                .clicked()
                            {
                                self.script_export();
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(
                                    self.source.is_some(),
                                    egui::Button::new("Save source snapshot..."),
                                )
                                .clicked()
                            {
                                self.snapshot_export(true);
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(
                                    self.target.is_some(),
                                    egui::Button::new("Save target snapshot..."),
                                )
                                .clicked()
                            {
                                self.snapshot_export(false);
                                ui.close_menu();
                            }
                        }
                        _ => {
                            if ui.button("About").clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Update (GitHub Releases)").clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(
                                    schema_compare::release_updates::RELEASES_URL,
                                ));
                                ui.close_menu();
                            }
                        }
                    }
                });
            }
        });
    }
    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_enabled_ui(self.pending.is_none(), |ui| {
            ui.horizontal(|ui| {
                if ui.button("Compare now / Refresh").clicked() {
                    self.start(ctx);
                }
                if ui
                    .add_enabled(self.source.is_some(), egui::Button::new("Review SQL"))
                    .clicked()
                {
                    self.tab = 1;
                    self.show_recent = false;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("grouping")
                        .width(144.0)
                        .selected_text(self.grouping.label())
                        .show_ui(ui, |ui| {
                            for g in [Grouping::Difference, Grouping::Object, Grouping::None] {
                                ui.selectable_value(&mut self.grouping, g, g.label());
                            }
                        });
                    ui.label("Group by:");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text("Search objects...")
                                .desired_width(180.0),
                        )
                        .changed()
                    {
                        self.select_filtered();
                    }
                });
            });
        });
    }
    fn source_target_strip(&self, ui: &mut egui::Ui, large: bool) {
        let height = if large { 80.0 } else { 65.0 };
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), height),
            egui::Sense::hover(),
        );
        let half = rect.center().x;
        let notch = height / 2.0;
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, BLUE);
        painter.add(egui::Shape::convex_polygon(
            vec![
                rect.left_top(),
                egui::pos2(half - notch, rect.top()),
                egui::pos2(half, rect.center().y),
                egui::pos2(half - notch, rect.bottom()),
                rect.left_bottom(),
            ],
            Color32::from_rgb(241, 241, 241),
            Stroke::NONE,
        ));
        let endpoints = [
            (&self.project.source, rect.left() + 18.0, TEXT, "Source"),
            (&self.project.target, half + 18.0, Color32::WHITE, "Target"),
        ];
        for (e, x, color, title) in endpoints {
            painter.text(
                egui::pos2(x, rect.top() + 10.0),
                egui::Align2::LEFT_TOP,
                title,
                egui::FontId::proportional(13.0),
                color,
            );
            let server = if e.snapshot {
                "Schema snapshot"
            } else {
                &e.connection.server
            };
            painter.text(
                egui::pos2(x, rect.top() + 28.0),
                egui::Align2::LEFT_TOP,
                server,
                egui::FontId::proportional(12.0),
                color,
            );
            let db = e.label();
            painter.text(
                egui::pos2(x, rect.top() + 45.0),
                egui::Align2::LEFT_TOP,
                if db.is_empty() {
                    "Select database"
                } else {
                    &db
                },
                egui::FontId::proportional(14.0),
                color,
            );
            if large {
                painter.text(
                    egui::pos2(x, rect.top() + 59.0),
                    egui::Align2::LEFT_TOP,
                    if e.snapshot {
                        "Snapshot file"
                    } else if e.connection.windows_auth {
                        "Windows authentication"
                    } else {
                        "SQL Server authentication"
                    },
                    egui::FontId::proportional(12.0),
                    color,
                );
            }
        }
    }
    fn rows(&self) -> Vec<ResultRow> {
        let query = self.query.to_lowercase();
        let filtered: Vec<_> = self
            .diffs
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                matches_filter(
                    d,
                    &self.status_filters,
                    &query,
                    &self.hidden_kinds,
                    self.show_identical,
                )
            })
            .map(|(i, _)| i)
            .collect();
        if self.grouping == Grouping::None {
            return filtered.into_iter().map(ResultRow::Object).collect();
        }
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for i in filtered {
            let d = &self.diffs[i];
            let key = match self.grouping {
                Grouping::Difference => status_group(&d.status).to_owned(),
                Grouping::Object => kind_group(&d.object().kind.to_string()),
                Grouping::None => unreachable!(),
            };
            groups.entry(key).or_default().push(i);
        }
        let mut rows = vec![];
        for (key, mut indices) in groups {
            indices.sort_by_key(|i| {
                let o = self.diffs[*i].object();
                (
                    status_group(&self.diffs[*i].status),
                    o.schema.clone(),
                    o.name.clone(),
                    o.kind.clone(),
                )
            });
            let collapsed = self.collapsed.contains(&key);
            rows.push(ResultRow::Group {
                key,
                indices: indices.clone(),
            });
            if !collapsed {
                rows.extend(indices.into_iter().map(ResultRow::Object));
            }
        }
        rows
    }
    fn results_table(&mut self, ui: &mut egui::Ui, height: f32) {
        let rows = self.rows();
        let mut select = None;
        let mut selection_changed = false;
        let mut toggle_group = None;
        let mut group_selection = None;
        ui.scope(|ui| {
            // Results select objects rather than text. Selectable labels otherwise
            // intercept row clicks and turn the cursor into a text-selection cursor.
            ui.style_mut().interaction.selectable_labels = false;
            TableBuilder::new(ui)
                .id_salt("results_table")
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click())
                .min_scrolled_height(height)
                .max_scroll_height(height)
                .column(Column::exact(30.0))
                .column(Column::initial(330.0).at_least(180.0))
                .column(Column::initial(90.0))
                .column(Column::initial(110.0))
                .column(Column::remainder().at_least(120.0))
                .column(Column::remainder().at_least(120.0))
                .header(26.0, |mut row| {
                    for label in ["", "Object", "Owner", "Type", "Source", "Target"] {
                        row.col(|ui| {
                            ui.strong(label);
                        });
                    }
                })
                .body(|body| {
                    body.rows(24.0, rows.len(), |mut row| match &rows[row.index()] {
                        ResultRow::Group { key, indices } => {
                            let selected_count =
                                indices.iter().filter(|i| self.diffs[**i].selected).count();
                            let eligible = indices
                                .iter()
                                .filter(|i| self.diffs[**i].status != Status::Equal)
                                .count();
                            row.set_selected(true);
                            row.col(|ui| {
                                let mut all = eligible > 0 && selected_count == eligible;
                                if ui
                                    .add_enabled(
                                        eligible > 0,
                                        egui::Checkbox::without_text(&mut all),
                                    )
                                    .changed()
                                {
                                    group_selection = Some((indices.clone(), all));
                                }
                            });
                            row.col(|ui| {
                                let title = key.split_once(':').map(|(_, s)| s).unwrap_or(key);
                                let arrow = if self.collapsed.contains(key) {
                                    "▶"
                                } else {
                                    "▼"
                                };
                                if ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(format!(
                                                "{arrow}  {title} ({})",
                                                indices.len()
                                            ))
                                            .strong(),
                                        )
                                        .sense(egui::Sense::click()),
                                    )
                                    .clicked()
                                {
                                    toggle_group = Some(key.clone());
                                }
                            });
                            row.col(|_| {});
                            row.col(|_| {});
                            row.col(|ui| {
                                ui.small(format!("{selected_count} selected"));
                            });
                            row.col(|_| {});
                        }
                        ResultRow::Object(index) => {
                            let d = &mut self.diffs[*index];
                            row.set_selected(self.selected == Some(*index));
                            row.col(|ui| {
                                selection_changed |= ui
                                    .add_enabled(
                                        d.status != Status::Equal,
                                        egui::Checkbox::without_text(&mut d.selected),
                                    )
                                    .changed();
                            });
                            let o = d.object();
                            let object_name = if let Some(parent) = &o.parent {
                                format!("{parent} / {}", o.name)
                            } else {
                                o.name.clone()
                            };
                            row.col(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(kind_symbol(&o.kind.to_string())).color(BLUE),
                                    );
                                    if object_name_label(ui, &object_name).clicked() {
                                        select = Some(*index);
                                    }
                                });
                            });
                            row.col(|ui| {
                                ui.label(&o.schema);
                            });
                            row.col(|ui| {
                                ui.label(o.kind.to_string());
                            });
                            row.col(|ui| {
                                ui.label(
                                    RichText::new(if d.source.is_some() {
                                        "■  Present"
                                    } else {
                                        "—"
                                    })
                                    .color(if d.source.is_some() { BLUE } else { MUTED }),
                                );
                            });
                            row.col(|ui| {
                                let label = match d.status {
                                    Status::Different => "≠  Different",
                                    Status::TargetOnly => "■  Target only",
                                    Status::SourceOnly => "—  Not exists",
                                    Status::Equal => "=  Identical",
                                };
                                ui.label(RichText::new(label).color(
                                    if d.status == Status::Different {
                                        Color32::from_rgb(148, 96, 0)
                                    } else {
                                        MUTED
                                    },
                                ));
                            });
                            if row.response().clicked() {
                                select = Some(*index);
                            }
                        }
                    });
                });
        });
        if let Some(key) = toggle_group {
            if !self.collapsed.remove(&key) {
                self.collapsed.insert(key);
            }
        }
        if let Some((indices, value)) = group_selection {
            for i in indices {
                if self.diffs[i].status != Status::Equal {
                    self.diffs[i].selected = value;
                }
            }
            selection_changed = true;
        }
        if let Some(i) = select {
            self.choose_object(i);
        }
        if selection_changed {
            self.rebuild();
        }
    }
    fn next_difference(&mut self, forward: bool) {
        let current = self.diff_cursor;
        let index = if forward {
            self.diff_rows
                .iter()
                .enumerate()
                .find(|(i, r)| current.is_none_or(|cursor| *i > cursor) && row_changed(r))
                .map(|(i, _)| i)
        } else {
            self.diff_rows
                .iter()
                .enumerate()
                .rev()
                .find(|(i, r)| current.is_none_or(|cursor| *i < cursor) && row_changed(r))
                .map(|(i, _)| i)
        };
        if let Some(i) = index {
            self.diff_cursor = Some(i);
            self.diff_jump = Some(i as f32 * 19.0);
        }
    }
    fn sql_diff(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.selected else {
            ui.centered_and_justified(|ui| {
                ui.label("Select an object to view its SQL differences.");
            });
            return;
        };
        let d = &self.diffs[index];
        let title = format!("{}  {}", d.object().kind, d.object().qualified());
        ui.horizontal(|ui| {
            ui.strong(title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Next difference ↓").clicked() {
                    self.next_difference(true);
                }
                if ui.small_button("Previous ↑").clicked() {
                    self.next_difference(false);
                }
            });
        });
        ui.columns(2, |cols| {
            cols[0].label(
                RichText::new(format!("Source: {}", self.project.source.label()))
                    .color(BLUE)
                    .strong(),
            );
            cols[1].label(
                RichText::new(format!("Target: {}", self.project.target.label()))
                    .color(BLUE)
                    .strong(),
            );
        });
        let half = (ui.available_width() / 2.0 - 3.0).max(50.0);
        let longest = self
            .diff_rows
            .iter()
            .flat_map(|r| [r.source.as_ref(), r.target.as_ref()])
            .flatten()
            .map(|l| l.text.chars().count())
            .max()
            .unwrap_or(0) as f32
            * 7.5;
        let max_horizontal = (longest + 65.0 - half).max(0.0);
        self.diff_horizontal = self.diff_horizontal.min(max_horizontal);
        if max_horizontal > 0.0 {
            ui.horizontal(|ui| {
                ui.small("Horizontal scroll");
                ui.add(
                    egui::Slider::new(&mut self.diff_horizontal, 0.0..=max_horizontal)
                        .show_value(false),
                );
            });
        }
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt(("sql_diff", index))
            .auto_shrink([false, false]);
        if let Some(jump) = self.diff_jump.take() {
            scroll = scroll.vertical_scroll_offset(jump);
        }
        let viewport_height = (ui.available_height() - 4.0).max(50.0);
        scroll.max_height(viewport_height).show_rows(
            ui,
            19.0,
            self.diff_rows.len(),
            |ui, range| {
                for i in range {
                    let r = &self.diff_rows[i];
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 19.0),
                        egui::Sense::hover(),
                    );
                    for (side, line) in [(0, &r.source), (1, &r.target)] {
                        let x = rect.left() + side as f32 * (half + 6.0);
                        let pane = egui::Rect::from_min_size(
                            egui::pos2(x, rect.top()),
                            Vec2::new(half, 19.0),
                        );
                        let bg = if line.as_ref().is_some_and(|l| l.changed) {
                            if r.source.is_some() && r.target.is_some() {
                                YELLOW
                            } else if side == 0 {
                                GREEN
                            } else {
                                RED
                            }
                        } else if line.is_none() {
                            Color32::from_rgb(239, 239, 239)
                        } else {
                            Color32::WHITE
                        };
                        ui.painter().rect_filled(pane, 0.0, bg);
                        ui.painter().line_segment(
                            [
                                pane.left_top() + Vec2::new(43.0, 0.0),
                                pane.left_bottom() + Vec2::new(43.0, 0.0),
                            ],
                            Stroke::new(1.0_f32, BORDER),
                        );
                        if let Some(l) = line {
                            ui.painter().text(
                                egui::pos2(x + 36.0, rect.top() + 2.0),
                                egui::Align2::RIGHT_TOP,
                                l.number.to_string(),
                                egui::FontId::monospace(12.0),
                                MUTED,
                            );
                            let text_rect = egui::Rect::from_min_max(
                                egui::pos2(x + 47.0, rect.top()),
                                pane.right_bottom(),
                            );
                            let painter = ui
                                .painter()
                                .with_clip_rect(text_rect.intersect(ui.clip_rect()));
                            let galley = ui.fonts(|fonts| fonts.layout_job(highlight_sql(&l.text)));
                            painter.galley(
                                egui::pos2(x + 50.0 - self.diff_horizontal, rect.top() + 1.0),
                                galley,
                                TEXT,
                            );
                        }
                    }
                }
            },
        );
    }
    fn lower_pane(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, 0, "SQL Differences");
            ui.selectable_value(
                &mut self.tab,
                1,
                format!("Deployment script ({})", self.actions),
            );
            ui.selectable_value(
                &mut self.tab,
                2,
                format!("Review ({})", self.blockers.len() + self.warnings.len()),
            );
        });
        ui.separator();
        match self.tab {
            0 => self.sql_diff(ui),
            1 => {
                let ready = self.blockers.is_empty() && self.actions > 0;
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(ready, egui::Button::new("Save SQL script..."))
                        .clicked()
                    {
                        self.script_export();
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Copy script"))
                        .clicked()
                    {
                        ui.ctx().copy_text(self.script.clone());
                    }
                    if ui
                        .add_enabled(
                            self.deploy_ready(),
                            egui::Button::new("Deploy to Target..."),
                        )
                        .clicked()
                    {
                        self.deploy_open = true;
                    }
                    ui.label(if self.blockers.is_empty() {
                        "Review the script before deploying or exporting"
                    } else {
                        "Script blocked — see Review"
                    });
                });
                let lines: Vec<_> = self.script.lines().collect();
                egui::ScrollArea::both()
                    .id_salt("deployment")
                    .auto_shrink([false, false])
                    .show_rows(ui, 18.0, lines.len(), |ui, range| {
                        for i in range {
                            ui.label(
                                RichText::new(format!("{:>4}  {}", i + 1, lines[i])).monospace(),
                            );
                        }
                    });
            }
            _ => {
                egui::ScrollArea::vertical()
                    .id_salt("review")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.blockers.is_empty() {
                            ui.label(
                                RichText::new("The selected objects have no planner blockers.")
                                    .color(Color32::DARK_GREEN),
                            );
                        }
                        for b in &self.blockers {
                            ui.label(
                                RichText::new(format!("Blocked: {b}"))
                                    .color(Color32::from_rgb(178, 37, 37)),
                            );
                        }
                        if !self.warnings.is_empty() {
                            ui.add_space(8.0);
                            ui.strong("Review notes");
                            for note in &self.warnings {
                                ui.label(note);
                                ui.add_space(5.0);
                            }
                        }
                    });
            }
        }
    }
    fn extra_windows(&mut self, ctx: &egui::Context) {
        self.about_panel.show(ctx, &mut self.about_open);
        if self.filters_open {
            let mut filter_changed = false;
            let kinds: BTreeSet<_> = self
                .diffs
                .iter()
                .map(|d| d.object().kind.to_string())
                .collect();
            egui::Window::new("Filter object types")
                .open(&mut self.filters_open)
                .default_width(320.0)
                .show(ctx, |ui| {
                    ui.label("Select the object types displayed in the results.");
                    for kind in &kinds {
                        let mut shown = !self.hidden_kinds.contains(kind);
                        if ui.checkbox(&mut shown, kind).changed() {
                            filter_changed = true;
                            if shown {
                                self.hidden_kinds.remove(kind);
                            } else {
                                self.hidden_kinds.insert(kind.clone());
                            }
                        }
                    }
                    if ui.button("Show all").clicked() {
                        self.hidden_kinds.clear();
                        filter_changed = true;
                    }
                    ui.label(
                        RichText::new(
                            "Filters select matching changes and clear hidden selections.",
                        )
                        .small()
                        .color(MUTED),
                    );
                });
            if filter_changed {
                self.select_filtered();
            }
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let mut events = vec![];
        let mut disconnected = false;
        if let Some(rx) = &self.pending {
            loop {
                match rx.try_recv() {
                    Ok(e) => events.push(e),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        let mut finished = false;
        for event in events {
            match event {
                Event::Progress(message) => {
                    if let Some(stage) = message.strip_prefix("Source: ") {
                        self.source_status = stage.into();
                    } else if let Some(stage) = message.strip_prefix("Target: ") {
                        self.target_status = stage.into();
                    } else {
                        self.message = message;
                    }
                }
                Event::Loaded(r) => {
                    finished = true;
                    self.pending = None;
                    match r {
                        Ok((s, t)) => self.accept(s, t),
                        Err(e) => self.set_message(Err(anyhow::anyhow!(e))),
                    }
                }
                Event::Imported(r) => {
                    finished = true;
                    self.pending = None;
                    match r {
                        Ok(p) => {
                            let path = p.redgate_path.clone();
                            self.apply_project(p, path);
                            self.message =
                                "Redgate project imported. Review Data sources, then compare."
                                    .into();
                        }
                        Err(e) => self.set_message(Err(anyhow::anyhow!(e))),
                    }
                }
                Event::Deployed(r) => {
                    finished = true;
                    self.pending = None;
                    let success = r.result.is_ok();
                    let refreshed = r.comparison.is_some();
                    if let Some((s, t)) = r.comparison {
                        self.accept(s, t);
                    } else {
                        self.comparison_stale = true;
                    }
                    self.tab = 0;
                    self.edit_open = false;
                    self.deploy_open = false;
                    self.query.clear();
                    self.collapsed.clear();
                    let mut detail=match r.result{Ok(())=>"Deployment completed. Selected changes were verified against Target. Compare shows the refreshed results.".into(),Err(e)=>e};
                    if !success && refreshed {
                        detail.push_str("\n\nCompare has been refreshed. Review the latest differences before trying again.");
                    }
                    if let Some(e) = r.refresh_error {
                        detail.push_str(&format!("\n\nCould not refresh Compare: {e}\nPrevious results are shown; run Compare again before deploying."));
                    }
                    self.message =
                        format!("{}: {detail}", if success { "Success" } else { "Fail" });
                    self.error = !success;
                    self.deployment_notice = Some((success, detail));
                }
            }
        }
        if disconnected && !finished {
            self.pending = None;
            self.set_message(Err(anyhow::anyhow!("Background task stopped unexpectedly")));
        }
        if self.pending.is_none() {
            self.pending_started = None;
        }
        if self.pending.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll(ctx);
        let discovery_active = self.edit_open && self.edit_tab == 0 && self.pending.is_none();
        self.database_browsers[0].tick(
            &self.project.source.connection,
            discovery_active && !self.project.source.snapshot,
        );
        self.database_browsers[1].tick(
            &self.project.target.connection,
            discovery_active && !self.project.target.snapshot,
        );
        if self.database_browsers.iter().any(DatabaseBrowser::waiting) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.loaded_connections.as_ref().is_some_and(|stamp| {
            *stamp
                != (
                    endpoint_stamp(&self.project.source),
                    endpoint_stamp(&self.project.target),
                )
        }) {
            self.clear_results();
            self.message =
                "Connection settings changed. Compare again to read the selected databases.".into();
        }
        if self.compare_on_start {
            self.compare_on_start = false;
            self.start(ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F5)) && self.pending.is_none() {
            self.start(ctx);
        }
        egui::TopBottomPanel::top("application_header")
            .frame(
                egui::Frame::new()
                    .fill(DARK)
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.image((self.brand_icon.id(), Vec2::splat(32.0)));
                    ui.label(
                        RichText::new("Schema Compare")
                            .color(Color32::WHITE)
                            .size(16.0),
                    );
                    if let Some(path) = &self.project_path {
                        ui.label(
                            RichText::new(path.file_name().unwrap_or_default().to_string_lossy())
                                .small()
                                .color(Color32::LIGHT_GRAY),
                        );
                    }
                });
                ui.add_enabled_ui(self.pending.is_none(), |ui| self.menus(ui, ctx));
            });
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(249, 249, 249))
                    .inner_margin(egui::Margin::symmetric(8, 5)),
            )
            .show(ctx, |ui| {
                self.toolbar(ui, ctx);
            });
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(244, 244, 244))
                    .inner_margin(egui::Margin::symmetric(8, 5)),
            )
            .show(ctx, |ui| {
                if self.error {
                    egui::ScrollArea::vertical()
                        .max_height(70.0)
                        .show(ui, |ui| {
                            ui.colored_label(Color32::from_rgb(178, 37, 37), &self.message);
                        });
                } else {
                    ui.horizontal(|ui| {
                        if self.pending.is_some() {
                            ui.spinner();
                        }
                        ui.label(&self.message);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.small(concat!("v", env!("CARGO_PKG_VERSION")));
                        });
                    });
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(8))
            .show(ctx, |ui| {
                self.source_target_strip(ui, false);
                ui.add_space(6.0);
                if self.pending.is_some() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.strong(&self.message);
                        });
                        if let Some(start) = self.pending_started {
                            ui.small(format!("Elapsed: {} seconds", start.elapsed().as_secs()));
                        }
                        if !self.source_status.is_empty(){ui.label(format!("Source: {}",self.source_status));}
                        if !self.target_status.is_empty(){ui.label(format!("Target: {}",self.target_status));}
                    });
                    ui.add_space(6.0);
                }
                if self.source.is_some() && !self.show_recent {
                    let selected = self
                        .diffs
                        .iter()
                        .filter(|d| d.selected && d.status != Status::Equal)
                        .count();
                    ui.horizontal(|ui| {
                        for (index,(status, label)) in [
                            (Status::Different, "Different"),
                            (Status::SourceOnly, "Source only"),
                            (Status::TargetOnly, "Target only"),
                            (Status::Equal, "Identical"),
                        ].into_iter().enumerate() {
                            let n = self.diffs.iter().filter(|d| d.status == status).count();
                            if index<3 {
                                if ui.selectable_label(self.status_filters[index],format!("{label}: {n}"))
                                    .on_hover_text("Toggle this status filter. Multiple statuses can be active; matching rows are selected.")
                                    .clicked() {
                                    self.status_filters[index] = !self.status_filters[index];
                                    self.select_filtered();
                                }
                            } else { ui.label(format!("{label}: {n}")); }
                            ui.separator();
                        }
                        ui.label(format!("{selected} selected"));
                        if ui.checkbox(&mut self.show_identical, "Show identical").changed() { self.select_filtered(); }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    self.deploy_ready(),
                                    egui::Button::new(
                                        RichText::new("Deploy...").color(Color32::WHITE),
                                    )
                                    .fill(BLUE),
                                )
                                .on_hover_text(
                                    "Execute selected schema changes on the live Target database",
                                )
                                .on_disabled_hover_text("Deploy requires a live Target, selected changes, and no blockers. See Review for unsupported migrations.")
                                .clicked()
                            {
                                self.deploy_open = true;
                            }
                            if ui.button("Select filtered").on_hover_text("Select only changes matching status, object type and search filters").clicked() {
                                self.select_filtered();
                            }
                            if ui.button("Clear").clicked() {
                                for d in &mut self.diffs {
                                    d.selected = false;
                                }
                                self.rebuild();
                            }
                        });
                    });
                    let max_height = (ui.available_height() - 170.0).max(100.0);
                    self.table_height = self.table_height.clamp(100.0, max_height);
                    ui.add_enabled_ui(self.pending.is_none(), |ui| {
                        self.results_table(ui, self.table_height);
                    });
                    let (bar, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 6.0),
                        egui::Sense::drag(),
                    );
                    ui.painter()
                        .rect_filled(bar, 0.0, Color32::from_rgb(231, 231, 231));
                    ui.painter().line_segment(
                        [
                            bar.center() - Vec2::new(20.0, 0.0),
                            bar.center() + Vec2::new(20.0, 0.0),
                        ],
                        Stroke::new(2.0_f32, BORDER),
                    );
                    if response.hovered() || response.dragged() {
                        ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    }
                    if response.drag_started() {
                        self.split_drag = true;
                    }
                    if response.dragged() && self.split_drag {
                        self.table_height = (self.table_height
                            + ctx.input(|i| i.pointer.delta().y))
                        .clamp(100.0, max_height);
                    }
                    if response.drag_stopped() {
                        self.split_drag = false;
                    }
                    self.lower_pane(ui);
                } else {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.heading(if self.pending.is_some() {
                            "Comparing data sources..."
                        } else {
                            "Start a comparison"
                        });
                        ui.add_space(8.0);
                        ui.label(
                            "Choose the source schema and the target schema you want to compare.",
                        );
                        ui.add_space(12.0);
                        ui.add_enabled_ui(self.pending.is_none(), |ui| {
                            ui.vertical_centered(|ui| {
                                if ui.button("Edit project...").clicked() {
                                    self.edit_open = true;
                                }
                            });
                        });
                        if self.pending.is_some() {
                            ui.add_space(12.0);
                            ui.spinner();
                        } else if !self.project.source.connection.database.is_empty() {
                            ui.add_space(16.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Compare now").color(Color32::WHITE),
                                    )
                                    .fill(BLUE),
                                )
                                .clicked()
                            {
                                self.start(ctx);
                            }
                        }
                    });
                    ui.add_space(18.0);
                    ui.add_enabled_ui(self.pending.is_none(),|ui|self.recent_panel(ui));
                }
            });
        self.edit_dialog(ctx);
        self.deploy_dialog(ctx);
        self.deployment_message(ctx);
        self.extra_windows(ctx);
    }
}
fn status_group(status: &Status) -> &'static str {
    match status {
        Status::Different => "0:Different objects",
        Status::SourceOnly => "1:Objects that exist only in the source",
        Status::TargetOnly => "2:Objects that exist only in the target",
        Status::Equal => "3:Identical objects",
    }
}
fn kind_group(kind: &str) -> String {
    let order = match kind {
        "Table" => 0,
        "Procedure" => 1,
        "View" => 2,
        "Function" => 3,
        "Trigger" => 4,
        "Index" => 5,
        "PrimaryKey" => 6,
        "Unique" => 7,
        "ForeignKey" => 8,
        "Check" => 9,
        "Schema" => 10,
        _ => 11,
    };
    format!("{order:02}:{kind}")
}
fn endpoint_stamp(endpoint: &Endpoint) -> String {
    serde_json::to_string(endpoint).unwrap_or_default()
}
fn kind_symbol(kind: &str) -> &'static str {
    match kind {
        "Table" => "▦",
        "View" => "▤",
        "Procedure" | "Function" => "ƒ",
        "Index" | "PrimaryKey" | "Unique" => "⌑",
        "Schema" => "□",
        _ => "◇",
    }
}
fn row_changed(r: &DiffRow) -> bool {
    r.source.as_ref().is_some_and(|l| l.changed) || r.target.as_ref().is_some_and(|l| l.changed)
}
fn highlight_sql(line: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut start = 0;
    let bytes = line.as_bytes();
    while start < bytes.len() {
        let mut end = start + 1;
        let mut color = TEXT;
        if bytes.get(start..start + 2) == Some(b"--") {
            end = bytes.len();
            color = Color32::from_rgb(0, 120, 48);
        } else if bytes[start] == b'\'' {
            color = Color32::from_rgb(156, 39, 43);
            while end < bytes.len() {
                if bytes[end] == b'\'' {
                    end += 1;
                    if bytes.get(end) == Some(&b'\'') {
                        end += 1;
                    } else {
                        break;
                    }
                } else {
                    end += 1;
                }
            }
        } else if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            let word = &line[start..end];
            if [
                "CREATE",
                "ALTER",
                "DROP",
                "TABLE",
                "VIEW",
                "PROCEDURE",
                "PROC",
                "FUNCTION",
                "TRIGGER",
                "SELECT",
                "FROM",
                "WHERE",
                "AS",
                "BEGIN",
                "END",
                "IF",
                "ELSE",
                "ON",
                "GO",
                "NULL",
                "NOT",
                "DEFAULT",
                "CONSTRAINT",
                "PRIMARY",
                "KEY",
                "FOREIGN",
                "REFERENCES",
                "SET",
                "INSERT",
                "UPDATE",
                "DELETE",
                "JOIN",
                "AND",
                "OR",
                "RETURN",
                "WITH",
                "ORDER",
                "BY",
                "ADD",
                "INDEX",
                "UNIQUE",
            ]
            .iter()
            .any(|w| word.eq_ignore_ascii_case(w))
            {
                color = Color32::from_rgb(0, 66, 173);
            }
        } else {
            while !line.is_char_boundary(end) {
                end += 1;
            }
        }
        job.append(
            &line[start..end],
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::monospace(13.0),
                color,
                ..Default::default()
            },
        );
        start = end;
    }
    job.wrap.max_width = f32::INFINITY;
    job
}
