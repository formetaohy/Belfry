use belfry_scene::Scene;

struct Snapshot {
    before: Scene,
    after: Scene,
}

#[derive(Default)]
pub(crate) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl History {
    pub(crate) fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub(crate) fn record(&mut self, before: Scene, after: Scene) {
        self.redo.clear();
        self.undo.push(Snapshot { before, after });
    }

    pub(crate) fn amend(&mut self, after: Scene) {
        self.undo
            .last_mut()
            .expect("an amended edit holds its undo step")
            .after = after;
    }

    pub(crate) fn undo(&mut self, scene: &mut Scene) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        *scene = snapshot.before.clone();
        self.redo.push(snapshot);
        true
    }

    pub(crate) fn redo(&mut self, scene: &mut Scene) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        *scene = snapshot.after.clone();
        self.undo.push(snapshot);
        true
    }
}
