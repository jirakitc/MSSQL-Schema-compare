use super::*;

fn connection_label(endpoint: &Endpoint) -> String {
    if endpoint.snapshot {
        format!("Snapshot · {}", endpoint.label())
    } else {
        endpoint.connection.database.clone()
    }
}
fn login_label(endpoint: &Endpoint) -> &str {
    if endpoint.snapshot {
        "Snapshot"
    } else if endpoint.connection.windows_auth {
        "Windows authentication"
    } else {
        &endpoint.connection.username
    }
}
impl App {
    pub(super) fn recent_panel(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        let mut clear = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Recent connections");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    clear = ui
                        .add_enabled(
                            !self.recent_connections.entries.is_empty(),
                            egui::Button::new("Clear"),
                        )
                        .on_hover_text(
                            "Clear the recent connection list; the current project stays unchanged",
                        )
                        .clicked();
                });
            });
            ui.small(
                "Reuse a recent Source → Target pair. SQL passwords are not saved in this list.",
            );
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.recent_search)
                        .id_salt("recent_search")
                        .hint_text(
                            RichText::new("Search recent connections (address or database)...")
                                .color(Color32::from_gray(155)),
                        )
                        .desired_width((ui.available_width() - 45.0).max(150.0)),
                );
                if ui
                    .add_enabled(!self.recent_search.is_empty(), egui::Button::new("×"))
                    .on_hover_text("Clear search")
                    .clicked()
                {
                    self.recent_search.clear();
                }
            });
            ui.add_space(8.0);
            let groups = self.recent_connections.filtered_groups(&self.recent_search);
            if self.recent_connections.entries.is_empty() {
                ui.label("No recent connections yet. Complete a comparison to add one.");
            } else if groups.is_empty() {
                ui.label("No matching recent connections.");
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("recent_connections_list")
                    .max_height((ui.available_height() - 10.0).clamp(70.0, 260.0))
                    .show(ui, |ui| {
                        for group in groups {
                            egui::CollapsingHeader::new(format!(
                                "Source Address: {}\nTarget Address: {}  ({} connections)",
                                group.source_address,
                                group.target_address,
                                group.indices.len()
                            ))
                            .id_salt((
                                "recent_addresses",
                                &group.source_address,
                                &group.target_address,
                            ))
                            .default_open(true)
                            .open(if self.recent_search.trim().is_empty() {
                                None
                            } else {
                                Some(true)
                            })
                            .show(ui, |ui| {
                                for index in group.indices {
                                    let project = &self.recent_connections.entries[index];
                                    ui.push_id(index, |ui| {
                                        egui::Frame::group(ui.style()).show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.set_max_width(
                                                        (ui.available_width() - 145.0).max(150.0),
                                                    );
                                                    ui.label(format!(
                                                        "Source: {}",
                                                        connection_label(&project.source)
                                                    ));
                                                    ui.label(format!(
                                                        "Target: {}",
                                                        connection_label(&project.target)
                                                    ));
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "Source login: {} · Target login: {}",
                                                            login_label(&project.source),
                                                            login_label(&project.target)
                                                        ))
                                                        .small()
                                                        .color(MUTED),
                                                    );
                                                });
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        if ui.button("Use connection").clicked() {
                                                            chosen = Some(index);
                                                        }
                                                    },
                                                );
                                            });
                                        });
                                    });
                                }
                            });
                        }
                    });
            }
        });
        if clear {
            let result = self
                .recent_connections
                .clear(&self.recent_path)
                .map(|_| "Recent connections cleared. The current project is unchanged.".into());
            if result.is_ok() {
                self.recent_search.clear();
            }
            self.set_message(result);
        } else if let Some(index) = chosen {
            let mut project = self.recent_connections.entries[index].clone();
            project.restore_redgate_credentials();
            self.apply_project(project, None);
            self.message="Recent Source/Target loaded. Review the connections and enter a SQL password if needed, then Compare.".into();
        }
    }
}
