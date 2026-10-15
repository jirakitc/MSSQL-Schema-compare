use super::*;
use schema_compare::release_updates::{self, ReleaseInfo};

#[derive(Default)]
pub(super) struct AboutPanel {
    result: Option<Result<Option<ReleaseInfo>, String>>,
    receiver: Option<Receiver<Result<Option<ReleaseInfo>, String>>>,
}
impl AboutPanel {
    fn check(&mut self, ctx: &egui::Context) {
        if self.receiver.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = release_updates::fetch_latest().map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn show(&mut self, ctx: &egui::Context, open: &mut bool) {
        if !*open {
            return;
        }
        if self.result.is_none() && self.receiver.is_none() {
            self.check(ctx);
        }
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    self.result = Some(result);
                    self.receiver = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.result = Some(Err("Release check was interrupted. Try again.".into()));
                    self.receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        egui::Window::new("About Schema Compare").open(open).resizable(false).default_width(540.0).show(ctx,|ui| {
            ui.heading("Schema Compare");
            ui.label("Compare SQL Server schemas, review SQL differences and deploy selected changes.");
            ui.add_space(12.0);
            ui.strong(concat!("Installed version: ",env!("CARGO_PKG_VERSION")));
            if self.receiver.is_some() {ui.horizontal(|ui|{ui.spinner();ui.label("Checking latest GitHub release...");});}
            else {match &self.result {
                Some(Ok(Some(release)))=>{
                    ui.label(format!("Latest GitHub release: {}{}",release.tag_name,if release.prerelease {" (pre-release)"} else {""}));
                    if release.tag_name.trim_start_matches('v')==env!("CARGO_PKG_VERSION") {ui.small("Installed version matches the latest published release.");}
                },
                Some(Ok(None))=>{ui.label("Latest GitHub release: no published releases yet.");},
                Some(Err(error))=>{ui.colored_label(Color32::from_rgb(178,37,37),error);},
                None=>{},
            }}
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Update").on_hover_text("Open GitHub Releases in your browser").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(release_updates::RELEASES_URL));
                }
                if ui.add_enabled(self.receiver.is_none(),egui::Button::new("Check again")).clicked() {self.check(ctx);}
            });
            ui.small("Update opens the release page. Download and extract the ZIP to update manually.");
            ui.separator();
            ui.hyperlink_to("GitHub repository",release_updates::REPOSITORY_URL);
            ui.small("No activation or licensing server. GitHub checks send only the application version, not database settings or passwords.");
        });
    }
}
