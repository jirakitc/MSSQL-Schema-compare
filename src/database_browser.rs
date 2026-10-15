use crate::catalog::{list_databases, Connection};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::mpsc::{self, Receiver},
};

pub fn connection_ready(c: &Connection) -> bool {
    !c.server.trim().is_empty()
        && (c.windows_auth || (!c.username.trim().is_empty() && !c.password.is_empty()))
}
fn fingerprint(c: &Connection) -> u64 {
    let mut h = DefaultHasher::new();
    (&c.server, c.windows_auth, c.trust_certificate).hash(&mut h);
    if !c.windows_auth {
        (&c.username, &c.password).hash(&mut h);
    }
    h.finish()
}
type Response = (u64, Result<Vec<String>, String>);
/// On-open discovery with stale-response rejection. Nothing here is persisted.
#[derive(Default)]
pub struct DatabaseBrowser {
    pub names: Vec<String>,
    pub search: String,
    pub error: Option<String>,
    pending: Option<(u64, Receiver<Response>)>,
    observed: Option<u64>,
    popup_open: bool,
}
impl DatabaseBrowser {
    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }
    pub fn waiting(&self) -> bool {
        self.loading()
    }
    fn observe(&mut self, c: &Connection) -> u64 {
        let stamp = fingerprint(c);
        if self.observed != Some(stamp) {
            self.observed = Some(stamp);
            self.names.clear();
            self.search.clear();
            self.error = None;
            self.pending = None;
        }
        stamp
    }
    pub fn popup(&mut self, c: &Connection, open: bool) {
        if open && !self.popup_open {
            self.refresh(c);
        }
        self.popup_open = open;
    }
    fn apply(&mut self, response: Response, current: u64) {
        let (stamp, result) = response;
        if stamp != current {
            return;
        }
        match result {
            Ok(names) => {
                self.names = names;
                self.error = None;
            }
            Err(error) => {
                self.names.clear();
                self.error = Some(error);
            }
        }
    }
    pub fn tick(&mut self, c: &Connection, active: bool) {
        let stamp = self.observe(c);
        if let Some((request_stamp, rx)) = &self.pending {
            let request_stamp = *request_stamp;
            match rx.try_recv() {
                Ok(response) => {
                    self.pending = None;
                    self.apply(response, stamp);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.apply(
                        (
                            request_stamp,
                            Err("Database discovery stopped unexpectedly".into()),
                        ),
                        stamp,
                    );
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !active {
            self.popup_open = false;
        }
    }
    pub fn refresh(&mut self, c: &Connection) {
        let stamp = self.observe(c);
        if self.pending.is_none() && connection_ready(c) {
            self.start(c, stamp);
        }
    }
    fn start(&mut self, c: &Connection, stamp: u64) {
        let c = c.clone();
        let (tx, rx) = mpsc::channel();
        self.pending = Some((stamp, rx));
        self.error = None;
        std::thread::spawn(move || {
            let result = list_databases(&c).map_err(|e| format!("{e:#}"));
            let _ = tx.send((stamp, result));
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disconnected_old_request_does_not_poison_new_credentials() {
        let mut browser = DatabaseBrowser::default();
        let mut connection = Connection::default();
        let old = browser.observe(&connection);
        let (tx, rx) = mpsc::channel();
        drop(tx);
        browser.pending = Some((old, rx));
        connection.server = "new-server".into();
        browser.tick(&connection, true);
        assert!(browser.error.is_none());
        assert!(!browser.waiting());
    }
    #[test]
    fn sql_credentials_must_be_complete() {
        let mut c = Connection {
            windows_auth: false,
            server: "server".into(),
            ..Default::default()
        };
        assert!(!connection_ready(&c));
        c.username = "dev".into();
        assert!(!connection_ready(&c));
        c.password = "private".into();
        assert!(connection_ready(&c));
        c.server.clear();
        assert!(!connection_ready(&c));
    }
    #[test]
    fn selected_database_does_not_invalidate_discovery() {
        let mut c = Connection::default();
        let original = fingerprint(&c);
        c.database = "DifferentDatabase".into();
        assert_eq!(original, fingerprint(&c));
        c.trust_certificate = true;
        assert_ne!(original, fingerprint(&c));
    }
    #[test]
    fn old_server_response_is_discarded() {
        let mut b = DatabaseBrowser::default();
        let mut c = Connection::default();
        let old = b.observe(&c);
        c.server = "other-server".into();
        let current = b.observe(&c);
        b.apply((old, Ok(vec!["private-old-db".into()])), current);
        assert!(b.names.is_empty());
        assert!(b.error.is_none());
        b.apply((current, Ok(vec!["new-db".into()])), current);
        assert_eq!(b.names, vec!["new-db"]);
    }
    #[test]
    fn typing_credentials_never_starts_discovery() {
        let mut b = DatabaseBrowser::default();
        let mut c = Connection::default();
        let stamp = b.observe(&c);
        b.apply((stamp, Err("Login failed".into())), stamp);
        b.tick(&c, true);
        assert!(!b.waiting());
        c.server = "changed-server".into();
        b.tick(&c, true);
        assert!(!b.waiting());
    }
}
