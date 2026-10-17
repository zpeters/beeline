//! A request's recent sends, kept for this session only. Pure: it stores
//! whatever the caller hands it and knows no clock.

/// How many sends a history keeps.
pub const LEN: usize = 20;

/// The sends, oldest first, at most `LEN`. Each gets a number that
/// counts up from 1 and is never reused, so a number still names the
/// same send after older ones drop off.
#[derive(Debug, Clone)]
pub struct History<T> {
    entries: Vec<(u64, T)>,
    next: u64,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next: 1,
        }
    }
}

impl<T> History<T> {
    /// Adds a send and returns its number.
    pub fn push(&mut self, item: T) -> u64 {
        let n = self.next;
        self.next += 1;
        self.entries.push((n, item));
        if self.entries.len() > LEN {
            self.entries.remove(0);
        }
        n
    }

    pub fn get(&self, n: u64) -> Option<&T> {
        self.entries.iter().find(|(m, _)| *m == n).map(|(_, t)| t)
    }

    /// The newest send's number.
    pub fn latest(&self) -> Option<u64> {
        self.entries.last().map(|(n, _)| *n)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Newest first, as the history list shows them.
    pub fn newest_first(&self) -> impl Iterator<Item = (u64, &T)> {
        self.entries.iter().rev().map(|(n, t)| (*n, t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_count_up_from_one() {
        let mut h = History::default();
        assert_eq!(h.latest(), None);
        assert_eq!(h.push("a"), 1);
        assert_eq!(h.push("b"), 2);
        assert_eq!(h.latest(), Some(2));
        assert_eq!(h.get(1), Some(&"a"));
    }

    #[test]
    fn newest_first_order() {
        let mut h = History::default();
        h.push("a");
        h.push("b");
        let order: Vec<_> = h.newest_first().collect();
        assert_eq!(order, vec![(2, &"b"), (1, &"a")]);
    }

    #[test]
    fn keeps_only_the_last_len_and_never_reuses_numbers() {
        let mut h = History::default();
        for i in 0..LEN + 5 {
            h.push(i);
        }
        assert_eq!(h.len(), LEN);
        assert_eq!(h.get(1), None);
        assert_eq!(h.get(6), Some(&5));
        assert_eq!(h.push(99), LEN as u64 + 6);
    }
}
