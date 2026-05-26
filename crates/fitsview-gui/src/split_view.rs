#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SplitLayout {
    #[default]
    Single,
    SideBySide,
    Grid2x2,
}

impl SplitLayout {
    pub fn pane_count(self) -> usize {
        match self {
            SplitLayout::Single => 1,
            SplitLayout::SideBySide => 2,
            SplitLayout::Grid2x2 => 4,
        }
    }
}

pub struct SplitView {
    pub layout: SplitLayout,
    /// Tab ID assigned to each pane slot. None means the pane is empty.
    pub pane_tab_ids: Vec<Option<u64>>,
    pub active_pane: usize,
    pub linked: bool,
}

impl Default for SplitView {
    fn default() -> Self {
        Self {
            layout: SplitLayout::Single,
            pane_tab_ids: vec![None],
            active_pane: 0,
            linked: false,
        }
    }
}

impl SplitView {
    pub fn set_layout(&mut self, layout: SplitLayout) {
        let old_ids = self.pane_tab_ids.clone();
        let n = layout.pane_count();
        self.pane_tab_ids = (0..n).map(|i| old_ids.get(i).copied().flatten()).collect();
        self.active_pane = self.active_pane.min(n.saturating_sub(1));
        self.layout = layout;
    }

    /// Assign the given tab id to the active pane.
    pub fn assign_active(&mut self, tab_id: u64) {
        if let Some(slot) = self.pane_tab_ids.get_mut(self.active_pane) {
            *slot = Some(tab_id);
        }
    }

    /// Remove a tab id from all pane slots.
    pub fn remove_tab(&mut self, tab_id: u64) {
        for slot in &mut self.pane_tab_ids {
            if *slot == Some(tab_id) {
                *slot = None;
            }
        }
    }

    /// Get the active tab id.
    pub fn active_tab_id(&self) -> Option<u64> {
        self.pane_tab_ids.get(self.active_pane).copied().flatten()
    }

    /// Fill empty panes with the given tab id.
    pub fn fill_empty(&mut self, tab_id: u64) {
        for slot in &mut self.pane_tab_ids {
            if slot.is_none() {
                *slot = Some(tab_id);
            }
        }
    }

    /// Return pane count.
    pub fn pane_count(&self) -> usize {
        self.layout.pane_count()
    }
}
