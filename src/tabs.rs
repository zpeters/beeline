//! An ordered list of open items with one active. Pure: the app decides
//! what an item holds; this only tracks order and focus.

#[derive(Debug)]
pub struct Tabs<T> {
    items: Vec<T>,
    active: usize,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            active: 0,
        }
    }
}

impl<T> Tabs<T> {
    /// Index of the active item, or None when there are no items.
    pub fn active_index(&self) -> Option<usize> {
        (!self.items.is_empty()).then_some(self.active)
    }

    pub fn active(&self) -> Option<&T> {
        self.items.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut T> {
        self.items.get_mut(self.active)
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.items.iter_mut()
    }

    /// Adds an item at the end and makes it active. Returns its index.
    pub fn push(&mut self, item: T) -> usize {
        self.items.push(item);
        self.active = self.items.len() - 1;
        self.active
    }

    /// Makes `index` active. Out-of-range indexes are ignored.
    pub fn activate(&mut self, index: usize) {
        if index < self.items.len() {
            self.active = index;
        }
    }

    /// The first index whose item matches `pred`.
    pub fn position(&self, pred: impl Fn(&T) -> bool) -> Option<usize> {
        self.items.iter().position(pred)
    }

    /// Removes the item at `index` and returns it. The item to its right
    /// becomes active if there is one, otherwise the one to its left.
    pub fn close(&mut self, index: usize) -> Option<T> {
        if index >= self.items.len() {
            return None;
        }
        let item = self.items.remove(index);
        if self.items.is_empty() {
            self.active = 0;
        } else if index < self.active {
            self.active -= 1;
        } else if self.active >= self.items.len() {
            self.active = self.items.len() - 1;
        }
        Some(item)
    }

    /// Removes every item.
    pub fn clear(&mut self) {
        self.items.clear();
        self.active = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> Tabs<&'static str> {
        let mut tabs = Tabs::default();
        tabs.push("a");
        tabs.push("b");
        tabs.push("c");
        tabs
    }

    #[test]
    fn push_makes_the_new_item_active() {
        let tabs = three();
        assert_eq!(tabs.iter().count(), 3);
        assert_eq!(tabs.active(), Some(&"c"));
        assert_eq!(tabs.active_index(), Some(2));
    }

    #[test]
    fn empty_has_no_active() {
        let tabs: Tabs<u8> = Tabs::default();
        assert_eq!(tabs.iter().count(), 0);
        assert_eq!(tabs.active(), None);
        assert_eq!(tabs.active_index(), None);
    }

    #[test]
    fn activate_ignores_out_of_range() {
        let mut tabs = three();
        tabs.activate(0);
        assert_eq!(tabs.active(), Some(&"a"));
        tabs.activate(9);
        assert_eq!(tabs.active(), Some(&"a"));
    }

    #[test]
    fn position_finds_first_match() {
        let tabs = three();
        assert_eq!(tabs.position(|s| *s == "b"), Some(1));
        assert_eq!(tabs.position(|s| *s == "z"), None);
    }

    #[test]
    fn closing_the_active_tab_activates_the_right_neighbour() {
        let mut tabs = three();
        tabs.activate(1);
        assert_eq!(tabs.close(1), Some("b"));
        assert_eq!(tabs.active(), Some(&"c"));
    }

    #[test]
    fn closing_the_last_tab_activates_the_left_neighbour() {
        let mut tabs = three();
        assert_eq!(tabs.close(2), Some("c"));
        assert_eq!(tabs.active(), Some(&"b"));
    }

    #[test]
    fn closing_a_tab_left_of_active_keeps_the_same_item_active() {
        let mut tabs = three();
        assert_eq!(tabs.close(0), Some("a"));
        assert_eq!(tabs.active(), Some(&"c"));
    }

    #[test]
    fn closing_a_tab_right_of_active_keeps_it_active() {
        let mut tabs = three();
        tabs.activate(0);
        tabs.close(2);
        assert_eq!(tabs.active(), Some(&"a"));
    }

    #[test]
    fn closing_the_only_tab_leaves_none_active() {
        let mut tabs = Tabs::default();
        tabs.push("x");
        assert_eq!(tabs.close(0), Some("x"));
        assert_eq!(tabs.iter().count(), 0);
        assert_eq!(tabs.active_index(), None);
    }

    #[test]
    fn close_out_of_range_is_none() {
        let mut tabs = three();
        assert_eq!(tabs.close(5), None);
        assert_eq!(tabs.iter().count(), 3);
    }

    #[test]
    fn clear_empties_the_list() {
        let mut tabs = three();
        tabs.clear();
        assert_eq!(tabs.iter().count(), 0);
    }
}
