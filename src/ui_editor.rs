use super::*;
use schema_compare::{database_browser::connection_ready, engine::Options};

fn database_combo_box(ui: &egui::Ui, selected: &str) -> egui::ComboBox {
    egui::ComboBox::from_id_salt("database_picker")
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(ui.available_width() - 12.0)
        .height(260.0)
        .selected_text(if selected.is_empty() {
            "Select database..."
        } else {
            selected
        })
}

fn project_window<'open>(ctx: &egui::Context, open: &'open mut bool) -> egui::Window<'open> {
    egui::Window::new("Edit project")
        .id(egui::Id::new("edit_project_v3"))
        .open(open)
        .collapsible(false)
        .movable(true)
        .resizable(true)
        .constrain(true)
        .default_pos(ctx.screen_rect().center() - Vec2::new(400.0, 230.0))
        .default_size([800.0, 460.0])
        .min_width(700.0)
        .max_width(840.0)
        .max_height(580.0)
}

impl App {
    fn endpoint_editor(
        ui: &mut egui::Ui,
        endpoint: &mut Endpoint,
        browser: &mut DatabaseBrowser,
        id: &str,
    ) {
        ui.push_id(id, |ui| {
            ui.horizontal(|ui| {
                ui.label("Data source");
                egui::ComboBox::from_id_salt("source_type")
                    .width(170.0)
                    .selected_text(if endpoint.snapshot {
                        "Snapshot file"
                    } else {
                        "Database"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut endpoint.snapshot, false, "Database");
                        ui.selectable_value(&mut endpoint.snapshot, true, "Snapshot file");
                    });
            });
            ui.add_space(6.0);
            if endpoint.snapshot {
                ui.strong("Snapshot file");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut endpoint.path)
                            .desired_width((ui.available_width() - 72.0).max(90.0)),
                    );
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Schema snapshot", &["json"])
                            .pick_file()
                        {
                            endpoint.path = path.display().to_string();
                        }
                    }
                });
                return;
            }
            ui.strong("Server");
            ui.add(
                egui::TextEdit::singleline(&mut endpoint.connection.server)
                    .desired_width(f32::INFINITY)
                    .hint_text("server,port or server\\instance"),
            );
            ui.add_space(7.0);
            let field_width = (ui.available_width() - 110.0).max(145.0);
            egui::Grid::new("authentication")
                .num_columns(2)
                .min_col_width(82.0)
                .spacing([8.0, 5.0])
                .show(ui, |ui| {
                    ui.label("Authentication");
                    egui::ComboBox::from_id_salt("auth")
                        .width(field_width)
                        .selected_text(if endpoint.connection.windows_auth {
                            "Windows authentication"
                        } else {
                            "SQL Server authentication"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut endpoint.connection.windows_auth,
                                true,
                                "Windows authentication",
                            );
                            ui.selectable_value(
                                &mut endpoint.connection.windows_auth,
                                false,
                                "SQL Server authentication",
                            );
                        });
                    ui.end_row();
                    ui.label("Username");
                    ui.add_enabled(
                        !endpoint.connection.windows_auth,
                        egui::TextEdit::singleline(&mut endpoint.connection.username)
                            .desired_width(field_width),
                    );
                    ui.end_row();
                    ui.label("Password");
                    ui.add_enabled(
                        !endpoint.connection.windows_auth,
                        egui::TextEdit::singleline(&mut endpoint.connection.password)
                            .password(true)
                            .desired_width(field_width),
                    );
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.checkbox(
                &mut endpoint.connection.trust_certificate,
                "Trust server certificate",
            );
            browser.tick(&endpoint.connection, true);
            ui.add_space(7.0);
            ui.strong("Database");
            let picker = database_combo_box(ui, &endpoint.connection.database).show_ui(ui, |ui| {
                ui.set_min_width(230.0);
                ui.set_min_height(240.0);
                ui.add(
                    egui::TextEdit::singleline(&mut browser.search)
                        .hint_text("Find database...")
                        .desired_width(230.0),
                );
                ui.separator();
                if !connection_ready(&endpoint.connection) {
                    ui.label("Enter server and login first.");
                } else if browser.loading() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading databases...");
                    });
                } else if browser.names.is_empty() {
                    ui.label("No accessible databases. Reopen to retry.");
                }
                let query = browser.search.to_lowercase();
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for name in &browser.names {
                            if (query.is_empty() || name.to_lowercase().contains(&query))
                                && ui
                                    .selectable_label(endpoint.connection.database == *name, name)
                                    .clicked()
                            {
                                endpoint.connection.database = name.clone();
                                ui.memory_mut(|memory| memory.close_popup());
                            }
                        }
                    });
            });
            browser.popup(&endpoint.connection, picker.inner.is_some());
            if browser.loading() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.small("Fetching database names...");
                });
            } else if let Some(error) = &browser.error {
                ui.colored_label(Color32::from_rgb(178, 37, 37), error);
            } else if !browser.names.is_empty() {
                ui.label(
                    RichText::new(format!("{} databases available", browser.names.len()))
                        .small()
                        .color(MUTED),
                );
            } else {
                ui.label(
                    RichText::new("Open the Database dropdown to load names")
                        .small()
                        .color(MUTED),
                );
            }
        });
    }
    fn ignore_options(ui: &mut egui::Ui, options: &mut Options) -> bool {
        let mut changed = false;
        ui.label("Choose comparison, column mapping and Ignore options. These affect comparison and deployment.");
        ui.add_space(5.0);
        ui.columns(2,|cols|{
            egui::Frame::group(cols[0].style()).show(&mut cols[0],|ui|{
                ui.strong("SQL text & module settings");
                changed|=ui.checkbox(&mut options.ignore_comments,"Ignore SQL comments").changed();
                changed|=ui.checkbox(&mut options.ignore_module_settings,"Ignore ANSI_NULLS / QUOTED_IDENTIFIER").on_hover_text("Retain the target module's SET options when its body is altered.").changed();
                ui.label(RichText::new("Token whitespace is always ignored; string literals are preserved.").small().color(MUTED));
            });
            egui::Frame::group(cols[0].style()).show(&mut cols[0],|ui|{
                ui.strong("Tables & columns");
                changed|=ui.checkbox(&mut options.auto_map_similar_columns,"Auto-map similar columns").on_hover_text("Map unambiguous similar column names with compatible types. Deployment renames Target columns to Source names and preserves values; review the mapping in SQL Differences.").changed();
                changed|=ui.checkbox(&mut options.ignore_column_order,"Ignore column order").changed();
                changed|=ui.checkbox(&mut options.ignore_column_name_case,"Ignore column name case").changed();
                changed|=ui.checkbox(&mut options.ignore_collation,"Ignore column collation").changed();
                changed|=ui.checkbox(&mut options.ignore_default_constraint_names,"Ignore DEFAULT constraint names").on_hover_text("Only the name is ignored; the default expression is still compared.").changed();
            });
            egui::Frame::group(cols[1].style()).show(&mut cols[1],|ui|{
                ui.strong("Indexes, constraints & triggers");
                changed|=ui.checkbox(&mut options.ignore_fill_factor,"Ignore index fill factor / padding").changed();
                changed|=ui.checkbox(&mut options.ignore_indexes,"Ignore indexes").on_hover_text("Exclude ordinary indexes; PRIMARY KEY and UNIQUE constraints stay in scope.").changed();
                changed|=ui.checkbox(&mut options.ignore_foreign_keys,"Ignore foreign-key constraints").changed();
                changed|=ui.checkbox(&mut options.ignore_check_constraints,"Ignore CHECK constraints").changed();
                changed|=ui.checkbox(&mut options.ignore_triggers,"Ignore DML triggers").changed();
                ui.label(RichText::new("Ignored types are excluded from compare/deployment. Dependency checks still protect retained objects.").small().color(MUTED));
            });
        });
        ui.add_space(8.0);
        ui.separator();
        ui.strong("Deployment");
        changed |= ui
            .checkbox(
                &mut options.allow_drops,
                "Allow destructive drops for selected target-only objects",
            )
            .changed();
        changed
    }
    pub(super) fn edit_dialog(&mut self, ctx: &egui::Context) {
        if !self.edit_open {
            return;
        }
        let mut open = true;
        let mut compare_clicked = false;
        let mut swap_clicked = false;
        let mut save_clicked = false;
        let mut cancel = false;
        let mut options_changed = false;
        project_window(ctx, &mut open).show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
            ui.spacing_mut().button_padding = Vec2::new(7.0, 3.0);
            for style in [egui::TextStyle::Body, egui::TextStyle::Button] {
                ui.style_mut()
                    .text_styles
                    .insert(style, egui::FontId::proportional(12.0));
            }
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.edit_tab, 0, "Data sources");
                ui.selectable_value(&mut self.edit_tab, 1, "Ignore options");
            });
            ui.separator();
            if self.edit_tab == 0 {
                self.source_target_strip(ui, true);
                ui.add_space(7.0);
            }
            egui::ScrollArea::vertical()
                .max_height(315.0)
                .auto_shrink([false, true])
                .show(ui, |ui| match self.edit_tab {
                    0 => ui.columns(2, |cols| {
                        Self::endpoint_editor(
                            &mut cols[0],
                            &mut self.project.source,
                            &mut self.database_browsers[0],
                            "source",
                        );
                        Self::endpoint_editor(
                            &mut cols[1],
                            &mut self.project.target,
                            &mut self.database_browsers[1],
                            "target",
                        );
                    }),
                    1 => {
                        options_changed = Self::ignore_options(ui, &mut self.project.options);
                    }
                    _ => {}
                });
            ui.separator();
            ui.horizontal(|ui| {
                save_clicked = ui.button("Save as...").clicked();
                swap_clicked = ui.button("Swap source / target").clicked();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    cancel = ui.button("Close").clicked();
                    compare_clicked = ui
                        .add_enabled(
                            self.pending.is_none(),
                            egui::Button::new(RichText::new("Compare now").color(Color32::WHITE))
                                .fill(BLUE),
                        )
                        .clicked();
                });
            });
        });
        self.edit_open = open && !cancel;
        if swap_clicked {
            self.swap();
            self.database_browsers = Default::default();
        }
        if save_clicked {
            self.project_io(true);
        }
        if options_changed && self.source.is_some() {
            self.recompare();
        }
        if compare_clicked {
            self.start(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct PickerFrame {
        button: egui::Rect,
        search: Option<egui::Rect>,
        item: Option<egui::Rect>,
        open: bool,
    }
    impl Default for PickerFrame {
        fn default() -> Self {
            Self {
                button: egui::Rect::NOTHING,
                search: None,
                item: None,
                open: false,
            }
        }
    }
    fn picker_frame(
        ctx: &egui::Context,
        query: &mut String,
        selected: &mut String,
        events: Vec<egui::Event>,
    ) -> PickerFrame {
        let mut frame = PickerFrame::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(800.0, 700.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let picker = database_combo_box(ui, selected).show_ui(ui, |ui| {
                        frame.search = Some(
                            ui.add(egui::TextEdit::singleline(query).hint_text("Find database..."))
                                .rect,
                        );
                        for name in ["AppDev", "AppProd"] {
                            if name.to_lowercase().contains(&query.to_lowercase()) {
                                let response = ui.selectable_label(*selected == name, name);
                                if name == "AppProd" {
                                    frame.item = Some(response.rect);
                                }
                                if response.clicked() {
                                    *selected = name.into();
                                    ui.memory_mut(|memory| memory.close_popup());
                                }
                            }
                        }
                    });
                    frame.button = picker.response.rect;
                    frame.open = egui::ComboBox::is_open(ctx, picker.response.id);
                });
            },
        );
        frame
    }
    fn picker_click(
        ctx: &egui::Context,
        query: &mut String,
        selected: &mut String,
        at: egui::Pos2,
    ) -> PickerFrame {
        picker_frame(
            ctx,
            query,
            selected,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        picker_frame(
            ctx,
            query,
            selected,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        )
    }
    #[test]
    fn database_search_stays_open_while_clicked_and_typed_then_closes_on_selection_or_outside_click(
    ) {
        let ctx = egui::Context::default();
        let mut query = String::new();
        let mut selected = "AppDev".into();
        let closed = picker_frame(&ctx, &mut query, &mut selected, vec![]);
        picker_click(&ctx, &mut query, &mut selected, closed.button.center());
        let open = picker_frame(&ctx, &mut query, &mut selected, vec![]);
        assert!(open.open);
        let focused = picker_click(
            &ctx,
            &mut query,
            &mut selected,
            open.search.unwrap().center(),
        );
        assert!(focused.open, "Clicking Find Database dismissed its popup");
        let typed = picker_frame(
            &ctx,
            &mut query,
            &mut selected,
            vec![egui::Event::Text("Prod".into())],
        );
        assert!(typed.open);
        assert_eq!(query, "Prod");
        let chosen = picker_click(
            &ctx,
            &mut query,
            &mut selected,
            typed.item.unwrap().center(),
        );
        assert_eq!(selected, "AppProd");
        assert!(!chosen.open, "Selecting a database should close the popup");
        picker_click(&ctx, &mut query, &mut selected, chosen.button.center());
        assert!(picker_frame(&ctx, &mut query, &mut selected, vec![]).open);
        assert!(
            !picker_click(&ctx, &mut query, &mut selected, egui::pos2(700.0, 500.0)).open,
            "An outside click should close the popup"
        );
    }
    fn frame(ctx: &egui::Context, events: Vec<egui::Event>) -> egui::Rect {
        let mut rect = egui::Rect::NOTHING;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1200.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                let mut open = true;
                rect = project_window(ctx, &mut open)
                    .show(ctx, |ui| {
                        ui.set_min_size(Vec2::new(760.0, 360.0));
                        ui.label("Data sources");
                    })
                    .unwrap()
                    .response
                    .rect;
            },
        );
        rect
    }
    #[test]
    fn compact_project_dialog_can_be_dragged_and_keeps_its_position() {
        let ctx = egui::Context::default();
        frame(&ctx, vec![]);
        let original = frame(&ctx, vec![]);
        assert!(
            original.width() < 840.0,
            "Dialog was not compact: {original:?}"
        );
        let from = egui::pos2(original.center().x, original.top() + 14.0);
        let to = from + Vec2::new(110.0, 60.0);
        frame(
            &ctx,
            vec![
                egui::Event::PointerMoved(from),
                egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(&ctx, vec![egui::Event::PointerMoved(to)]);
        frame(
            &ctx,
            vec![egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        let moved = frame(&ctx, vec![]);
        assert!(
            moved.left() > original.left() + 50.0 && moved.top() > original.top() + 30.0,
            "Dialog did not move: {original:?} -> {moved:?}"
        );
        assert_eq!(
            frame(&ctx, vec![]).min,
            moved.min,
            "Dialog snapped back to its anchor"
        );
    }
    #[test]
    fn connection_editors_do_not_expand_the_dialog_each_frame() {
        let ctx = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        let mut source = Endpoint::default();
        source.connection.server.clear();
        let mut target = source.clone();
        let mut browsers: [DatabaseBrowser; 2] = Default::default();
        for _ in 0..20 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1380.0, 860.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    let mut open = true;
                    rect = project_window(ctx, &mut open)
                        .show(ctx, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                            ui.spacing_mut().button_padding = Vec2::new(7.0, 3.0);
                            for style in [egui::TextStyle::Body, egui::TextStyle::Button] {
                                ui.style_mut()
                                    .text_styles
                                    .insert(style, egui::FontId::proportional(12.0));
                            }
                            ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 80.0),
                                egui::Sense::hover(),
                            );
                            egui::ScrollArea::vertical()
                                .max_height(315.0)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.columns(2, |cols| {
                                        App::endpoint_editor(
                                            &mut cols[0],
                                            &mut source,
                                            &mut browsers[0],
                                            "source",
                                        );
                                        App::endpoint_editor(
                                            &mut cols[1],
                                            &mut target,
                                            &mut browsers[1],
                                            "target",
                                        );
                                    });
                                });
                        })
                        .unwrap()
                        .response
                        .rect;
                },
            );
        }
        assert!(
            rect.width() < 880.0,
            "Editor grew wider than its compact size: {rect:?}"
        );
    }
}
