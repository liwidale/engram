use crate::error::{EngramError, Result};
use crate::schema::EngramDocument;
use crate::store::{Snapshot, Store};
use serde::Serialize;

pub const HISTORY_LIMIT: usize = 30;

struct Step {
    document: EngramDocument,
    label: String,
}

#[derive(Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}

impl History {
    pub fn record(&mut self, before: EngramDocument, after: &Snapshot, label: impl Into<String>) {
        if before == after.document {
            return;
        }
        self.undo.push(Step {
            document: before,
            label: label.into(),
        });
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision = Some(after.document.revision);
    }

    pub fn state(&self) -> HistoryState {
        HistoryState {
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            undo_label: self.undo.last().map(|s| s.label.clone()),
            redo_label: self.redo.last().map(|s| s.label.clone()),
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.revision = None;
    }

    pub fn undo(&mut self, store: &Store, current: &EngramDocument) -> Result<Snapshot> {
        self.step(store, current, true)
    }

    pub fn redo(&mut self, store: &Store, current: &EngramDocument) -> Result<Snapshot> {
        self.step(store, current, false)
    }

    fn step(&mut self, store: &Store, current: &EngramDocument, undo: bool) -> Result<Snapshot> {
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(step) = from.pop() else {
            return Err(EngramError::Conflict(format!(
                "nothing to {}",
                if undo { "undo" } else { "redo" }
            )));
        };
        if self.revision.is_some_and(|r| r != current.revision) {
            from.push(step);
            return Err(EngramError::Conflict(
                "the file changed outside Engram since your last edit, so undo would overwrite those changes".into(),
            ));
        }
        match store.replace(step.document.clone(), Some(current.revision)) {
            Ok(snap) => {
                to.push(Step {
                    document: current.clone(),
                    label: step.label,
                });
                self.revision = Some(snap.document.revision);
                Ok(snap)
            }
            Err(e) => {
                from.push(step);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::Op;
    use serde_json::json;

    fn run(store: &Store, history: &mut History, v: serde_json::Value) -> Snapshot {
        let before = store.load().unwrap().document;
        let (_, snap) = serde_json::from_value::<Op>(v).unwrap().execute(store).unwrap();
        history.record(before, &snap, "edit");
        snap
    }

    #[test]
    fn undo_and_redo_through_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("p.engram"));
        store.create("P").unwrap();
        let mut h = History::default();
        run(
            &store,
            &mut h,
            json!({"op":"upsert_memory","key":"a","category":"rule","content":"x"}),
        );
        let snap = run(&store, &mut h, json!({"op":"delete_memory","key":"a"}));
        assert!(h.state().can_undo);

        let undone = h.undo(&store, &snap.document).unwrap();
        assert!(undone.document.memory_entry("a").is_some());
        assert!(
            undone.document.revision > snap.document.revision,
            "revisions stay monotonic"
        );
        assert!(h.state().can_redo);
        let redone = h.redo(&store, &undone.document).unwrap();
        assert!(redone.document.memory_entry("a").is_none());
        assert_eq!(store.load().unwrap().document.revision, redone.document.revision);
    }

    #[test]
    fn refuses_to_overwrite_external_edits() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("p.engram"));
        store.create("P").unwrap();
        let mut h = History::default();
        run(
            &store,
            &mut h,
            json!({"op":"upsert_memory","key":"mine","category":"rule","content":"x"}),
        );
        let (_, external) =
            serde_json::from_value::<Op>(json!({"op":"upsert_memory","key":"agent","category":"rule","content":"y"}))
                .unwrap()
                .execute(&store)
                .unwrap();
        let err = h.undo(&store, &external.document).unwrap_err();
        assert_eq!(err.code(), "conflict");
        assert!(store.load().unwrap().document.memory_entry("agent").is_some());
        assert!(h.state().can_undo, "the step stays available");
    }

    #[test]
    fn keeps_at_most_thirty_steps() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("p.engram"));
        store.create("P").unwrap();
        let mut h = History::default();
        for i in 0..35 {
            run(
                &store,
                &mut h,
                json!({"op":"upsert_memory","key":format!("k{i}"),"category":"note","content":"x"}),
            );
        }
        assert_eq!(h.undo.len(), HISTORY_LIMIT);
    }
}
