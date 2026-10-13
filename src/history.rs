//! Undo/redo for the annotation set.

use crate::model::AnnotationSet;

/// Maximum number of remembered states.
pub const DEFAULT_LIMIT: usize = 128;

/// A bounded undo/redo stack of whole annotation-set snapshots.
///
/// The data sets produced by this app are small (a few thousand annotations at
/// most), so snapshots are simpler and more reliable than command objects.
#[derive(Debug, Clone)]
pub struct History {
    undo: Vec<AnnotationSet>,
    redo: Vec<AnnotationSet>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::with_limit(DEFAULT_LIMIT)
    }
}

impl History {
    /// Creates an empty history with the given capacity.
    #[must_use]
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Number of states that can be undone.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Number of states that can be redone.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// `true` when there is nothing to undo.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// `true` when at least one state can be undone.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// `true` when at least one state can be redone.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Records `state` as the state *before* a mutation.
    pub fn record(&mut self, state: &AnnotationSet) {
        self.undo.push(state.clone());
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Restores the previous state. Returns `true` if something was restored.
    pub fn undo(&mut self, state: &mut AnnotationSet) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(state, previous));
        true
    }

    /// Restores the state that was undone last. Returns `true` on success.
    pub fn redo(&mut self, state: &mut AnnotationSet) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(state, next));
        true
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AnnotationSet;
    use egui::pos2;

    fn set_with(n: usize) -> AnnotationSet {
        let mut set = AnnotationSet::new();
        for _ in 0..n {
            set.add_point(pos2(0.0, 0.0), None);
        }
        set
    }

    #[test]
    fn undo_and_redo_round_trip() {
        let mut history = History::with_limit(8);
        let mut set = AnnotationSet::new();

        history.record(&set);
        set.add_point(pos2(1.0, 1.0), None);
        assert_eq!(set.len(), 1);

        history.record(&set);
        set.add_point(pos2(2.0, 2.0), None);
        assert_eq!(set.len(), 2);

        assert!(history.undo(&mut set));
        assert_eq!(set.len(), 1);
        assert!(history.undo(&mut set));
        assert_eq!(set.len(), 0);
        assert!(!history.undo(&mut set));

        assert!(history.redo(&mut set));
        assert_eq!(set.len(), 1);
        assert!(history.redo(&mut set));
        assert_eq!(set.len(), 2);
        assert!(!history.redo(&mut set));
    }

    #[test]
    fn recording_clears_the_redo_stack() {
        let mut history = History::with_limit(8);
        let mut set = AnnotationSet::new();
        history.record(&set);
        set.add_point(pos2(0.0, 0.0), None);
        history.undo(&mut set);
        assert_eq!(history.redo_len(), 1);

        history.record(&set);
        assert_eq!(history.redo_len(), 0);
        assert!(!history.redo(&mut set));
    }

    #[test]
    fn limit_drops_the_oldest_states() {
        let mut history = History::with_limit(2);
        let mut set = AnnotationSet::new();
        for n in 1..=4 {
            history.record(&set);
            set = set_with(n);
        }
        assert_eq!(history.undo_len(), 2);
        assert!(history.undo(&mut set));
        assert_eq!(set.len(), 3);
        assert!(history.undo(&mut set));
        assert_eq!(set.len(), 2);
        assert!(!history.undo(&mut set));
    }

    #[test]
    fn clear_and_counts() {
        let mut history = History::with_limit(4);
        let mut set = AnnotationSet::new();
        history.record(&set);
        assert_eq!(history.undo_len(), 1);
        assert!(!history.is_empty());
        history.clear();
        assert!(history.is_empty());
        assert_eq!(history.redo_len(), 0);
        assert!(!history.undo(&mut set));
    }

    #[test]
    fn default_limit_is_used() {
        let mut history = History::default();
        let mut set = AnnotationSet::new();
        for n in 1..=(DEFAULT_LIMIT + 10) {
            history.record(&set);
            set = set_with(n);
        }
        assert_eq!(history.undo_len(), DEFAULT_LIMIT);
    }
}
