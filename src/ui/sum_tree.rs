//! Copy-on-write `Arc`-backed B-tree (`SumTree`) indexed by monoid summaries (`Summary`).
//!
//! Inspired by Zed's `gpui::SumTree` / `xi-rope`: stores finalized UI timeline cells in
//! fixed-fanout nodes whose cached monoid summaries allow $O(\log N)$ seeking by visual row,
//! $O(\log N + K)$ viewport range iteration without touching off-screen items, and $O(\log N)$
//! reverse sticky-anchor resolution.

use std::sync::Arc;

/// Maximum children/items per B-tree node (`NODE_CAP = 8` keeps internal summaries in 1–2 CPU cache lines).
const NODE_CAP: usize = 8;
const MIN_CHILDREN: usize = NODE_CAP / 2;

/// Associative monoid summary aggregated at every `SumTree` node.
pub trait Summary: Clone + Default + std::fmt::Debug {
    /// Combines two adjacent summaries (`combine(a, combine(b, c)) == combine(combine(a, b), c)`).
    fn combine(left: &Self, right: &Self) -> Self;
}

/// Items stored at the leaves of a `SumTree`.
pub trait Summarizable: Clone {
    type Summary: Summary;

    fn summary(&self) -> Self::Summary;
}

/// Monotonic `usize` projection over a `Summary` (e.g., total visual line count).
pub trait Dimension<S: Summary> {
    fn measure(summary: &S) -> usize;
}

#[derive(Debug, Clone)]
enum Node<T: Summarizable> {
    Leaf {
        summary: T::Summary,
        items: Vec<T>,
    },
    Internal {
        summary: T::Summary,
        len: usize,
        children: Vec<Arc<Node<T>>>,
    },
}

impl<T: Summarizable> Node<T> {
    fn empty_leaf() -> Self {
        Self::Leaf {
            summary: T::Summary::default(),
            items: Vec::new(),
        }
    }

    #[inline]
    fn summary(&self) -> &T::Summary {
        match self {
            Self::Leaf { summary, .. } | Self::Internal { summary, .. } => summary,
        }
    }

    #[inline]
    fn item_len(&self) -> usize {
        match self {
            Self::Leaf { items, .. } => items.len(),
            Self::Internal { len, .. } => *len,
        }
    }

    #[inline]
    fn is_full(&self) -> bool {
        match self {
            Self::Leaf { items, .. } => items.len() >= NODE_CAP,
            Self::Internal { children, .. } => children.len() >= NODE_CAP,
        }
    }

    fn recompute_leaf(items: Vec<T>) -> Self {
        let mut summary = T::Summary::default();
        for item in &items {
            summary = T::Summary::combine(&summary, &item.summary());
        }
        Self::Leaf { summary, items }
    }

    fn recompute_internal(children: Vec<Arc<Node<T>>>) -> Self {
        let mut summary = T::Summary::default();
        let mut len = 0usize;
        for child in &children {
            summary = T::Summary::combine(&summary, child.summary());
            len += child.item_len();
        }
        Self::Internal {
            summary,
            len,
            children,
        }
    }

    fn split(&mut self) -> Node<T> {
        match self {
            Self::Leaf { items, summary } => {
                let right_items = items.split_off(MIN_CHILDREN);
                let mut left_sum = T::Summary::default();
                for it in items.iter() {
                    left_sum = T::Summary::combine(&left_sum, &it.summary());
                }
                *summary = left_sum;
                Self::recompute_leaf(right_items)
            }
            Self::Internal {
                children,
                summary,
                len,
            } => {
                let right_children = children.split_off(MIN_CHILDREN);
                let mut left_sum = T::Summary::default();
                let mut left_len = 0usize;
                for ch in children.iter() {
                    left_sum = T::Summary::combine(&left_sum, ch.summary());
                    left_len += ch.item_len();
                }
                *summary = left_sum;
                *len = left_len;
                Self::recompute_internal(right_children)
            }
        }
    }

    fn push_recursive(&mut self, item: T) -> Option<Node<T>> {
        match self {
            Self::Leaf { items, summary } => {
                let item_sum = item.summary();
                *summary = T::Summary::combine(summary, &item_sum);
                items.push(item);
                if items.len() > NODE_CAP {
                    Some(self.split())
                } else {
                    None
                }
            }
            Self::Internal {
                children,
                summary,
                len,
            } => {
                let Some(last_arc) = children.last_mut() else {
                    let leaf = Self::recompute_leaf(vec![item]);
                    *summary = leaf.summary().clone();
                    *len = 1;
                    children.push(Arc::new(leaf));
                    return None;
                };
                let split_opt = Arc::make_mut(last_arc).push_recursive(item);
                if let Some(right_node) = split_opt {
                    children.push(Arc::new(right_node));
                }
                let mut new_sum = T::Summary::default();
                let mut new_len = 0usize;
                for ch in children.iter() {
                    new_sum = T::Summary::combine(&new_sum, ch.summary());
                    new_len += ch.item_len();
                }
                *summary = new_sum;
                *len = new_len;
                if children.len() > NODE_CAP {
                    Some(self.split())
                } else {
                    None
                }
            }
        }
    }

    fn set_recursive(&mut self, index: usize, item: T) -> bool {
        match self {
            Self::Leaf { items, summary } => {
                if index >= items.len() {
                    return false;
                }
                items[index] = item;
                let mut new_sum = T::Summary::default();
                for it in items.iter() {
                    new_sum = T::Summary::combine(&new_sum, &it.summary());
                }
                *summary = new_sum;
                true
            }
            Self::Internal {
                children, summary, ..
            } => {
                let mut rem = index;
                let mut updated = false;
                for child_arc in children.iter_mut() {
                    let c_len = child_arc.item_len();
                    if rem < c_len {
                        updated = Arc::make_mut(child_arc).set_recursive(rem, item);
                        break;
                    }
                    rem -= c_len;
                }
                if updated {
                    let mut new_sum = T::Summary::default();
                    for ch in children.iter() {
                        new_sum = T::Summary::combine(&new_sum, ch.summary());
                    }
                    *summary = new_sum;
                }
                updated
            }
        }
    }

    fn get_recursive(&self, index: usize) -> Option<&T> {
        match self {
            Self::Leaf { items, .. } => items.get(index),
            Self::Internal { children, .. } => {
                let mut rem = index;
                for ch in children {
                    let c_len = ch.item_len();
                    if rem < c_len {
                        return ch.get_recursive(rem);
                    }
                    rem -= c_len;
                }
                None
            }
        }
    }

    fn truncate_recursive(&mut self, keep_len: usize) {
        if keep_len == 0 {
            *self = Self::empty_leaf();
            return;
        }
        match self {
            Self::Leaf { items, summary } => {
                if keep_len < items.len() {
                    items.truncate(keep_len);
                    let mut new_sum = T::Summary::default();
                    for it in items.iter() {
                        new_sum = T::Summary::combine(&new_sum, &it.summary());
                    }
                    *summary = new_sum;
                }
            }
            Self::Internal {
                children,
                summary,
                len,
            } => {
                let mut rem = keep_len;
                let mut keep_children = 0usize;
                for child_arc in children.iter_mut() {
                    let c_len = child_arc.item_len();
                    if rem == 0 {
                        break;
                    }
                    keep_children += 1;
                    if rem < c_len {
                        Arc::make_mut(child_arc).truncate_recursive(rem);
                        rem = 0;
                    } else {
                        rem -= c_len;
                    }
                }
                children.truncate(keep_children);
                let mut new_sum = T::Summary::default();
                let mut new_len = 0usize;
                for ch in children.iter() {
                    new_sum = T::Summary::combine(&new_sum, ch.summary());
                    new_len += ch.item_len();
                }
                *summary = new_sum;
                *len = new_len;
            }
        }
    }

    fn visit_dimension_range<D, F>(
        &self,
        start_dim: usize,
        end_dim: usize,
        cur_item_idx: &mut usize,
        cur_dim: &mut usize,
        visitor: &mut F,
    ) where
        D: Dimension<T::Summary>,
        F: FnMut(usize, usize, &T),
    {
        if *cur_dim >= end_dim {
            return;
        }
        let node_span = D::measure(self.summary());
        let node_end = *cur_dim + node_span;
        if node_end <= start_dim {
            *cur_dim = node_end;
            *cur_item_idx += self.item_len();
            return;
        }
        match self {
            Self::Leaf { items, .. } => {
                for item in items {
                    if *cur_dim >= end_dim {
                        return;
                    }
                    let item_span = D::measure(&item.summary());
                    let item_end = *cur_dim + item_span;
                    if item_end > start_dim {
                        visitor(*cur_item_idx, *cur_dim, item);
                    }
                    *cur_dim = item_end;
                    *cur_item_idx += 1;
                }
            }
            Self::Internal { children, .. } => {
                for ch in children {
                    if *cur_dim >= end_dim {
                        return;
                    }
                    ch.visit_dimension_range::<D, F>(
                        start_dim,
                        end_dim,
                        cur_item_idx,
                        cur_dim,
                        visitor,
                    );
                }
            }
        }
    }

    fn rfind_before_or_after<D, P, M>(
        &self,
        base_item_idx: usize,
        base_dim: usize,
        subtree_predicate: &P,
        item_matcher: &M,
    ) -> Option<(usize, usize, &T)>
    where
        D: Dimension<T::Summary>,
        P: Fn(usize, &T::Summary) -> bool,
        M: Fn(usize, &T) -> bool,
    {
        if !subtree_predicate(base_dim, self.summary()) {
            return None;
        }
        match self {
            Self::Leaf { items, .. } => {
                let mut offsets = [0usize; NODE_CAP + 1];
                let mut acc = base_dim;
                for (i, it) in items.iter().enumerate() {
                    offsets[i] = acc;
                    acc += D::measure(&it.summary());
                }
                for (i, it) in items.iter().enumerate().rev() {
                    let item_start = offsets[i];
                    if item_matcher(item_start, it) {
                        return Some((base_item_idx + i, item_start, it));
                    }
                }
                None
            }
            Self::Internal { children, .. } => {
                let mut child_dims = [0usize; NODE_CAP + 1];
                let mut child_idxs = [0usize; NODE_CAP + 1];
                let mut d_acc = base_dim;
                let mut i_acc = base_item_idx;
                for (i, ch) in children.iter().enumerate() {
                    child_dims[i] = d_acc;
                    child_idxs[i] = i_acc;
                    d_acc += D::measure(ch.summary());
                    i_acc += ch.item_len();
                }
                for (i, ch) in children.iter().enumerate().rev() {
                    if let Some(found) = ch.rfind_before_or_after::<D, P, M>(
                        child_idxs[i],
                        child_dims[i],
                        subtree_predicate,
                        item_matcher,
                    ) {
                        return Some(found);
                    }
                }
                None
            }
        }
    }

    fn find_first<D, P, M>(
        &self,
        base_item_idx: usize,
        base_dim: usize,
        subtree_predicate: &P,
        item_matcher: &M,
    ) -> Option<(usize, usize, &T)>
    where
        D: Dimension<T::Summary>,
        P: Fn(usize, &T::Summary) -> bool,
        M: Fn(usize, usize, &T) -> bool,
    {
        if !subtree_predicate(base_dim, self.summary()) {
            return None;
        }
        match self {
            Self::Leaf { items, .. } => {
                let mut d_acc = base_dim;
                for (i, it) in items.iter().enumerate() {
                    let idx = base_item_idx + i;
                    let start = d_acc;
                    if item_matcher(idx, start, it) {
                        return Some((idx, start, it));
                    }
                    d_acc += D::measure(&it.summary());
                }
                None
            }
            Self::Internal { children, .. } => {
                let mut d_acc = base_dim;
                let mut i_acc = base_item_idx;
                for ch in children {
                    if let Some(found) =
                        ch.find_first::<D, P, M>(i_acc, d_acc, subtree_predicate, item_matcher)
                    {
                        return Some(found);
                    }
                    d_acc += D::measure(ch.summary());
                    i_acc += ch.item_len();
                }
                None
            }
        }
    }
}

/// Copy-on-write `Arc` B-tree indexed by monoid `Summary`.
#[derive(Debug, Clone)]
pub struct SumTree<T: Summarizable> {
    root: Arc<Node<T>>,
}

impl<T: Summarizable> Default for SumTree<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Summarizable> SumTree<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: Arc::new(Node::empty_leaf()),
        }
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.root.item_len()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    #[must_use]
    pub fn summary(&self) -> &T::Summary {
        self.root.summary()
    }

    pub fn clear(&mut self) {
        if !self.is_empty() {
            self.root = Arc::new(Node::empty_leaf());
        }
    }

    /// Appends `item` to the end of the tree in $O(\log N)$ time.
    pub fn push(&mut self, item: T) {
        let split_opt = Arc::make_mut(&mut self.root).push_recursive(item);
        if let Some(right_node) = split_opt {
            let left_arc = Arc::clone(&self.root);
            let right_arc = Arc::new(right_node);
            self.root = Arc::new(Node::recompute_internal(vec![left_arc, right_arc]));
        } else if self.root.is_full() {
            // Keep invariant clean without premature split until > NODE_CAP.
        }
    }

    /// Replaces the item at `index` in $O(\log N)$ time, recomputing only ancestor summaries.
    pub fn set(&mut self, index: usize, item: T) -> bool {
        Arc::make_mut(&mut self.root).set_recursive(index, item)
    }

    /// Retrieves the item at `index` in $O(\log N)$ time.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&T> {
        self.root.get_recursive(index)
    }

    /// Truncates the tree to the first `keep_len` items in $O(\log N)$ time.
    pub fn truncate(&mut self, keep_len: usize) {
        if keep_len >= self.len() {
            return;
        }
        if keep_len == 0 {
            self.clear();
            return;
        }
        Arc::make_mut(&mut self.root).truncate_recursive(keep_len);
        while let Node::Internal { children, .. } = self.root.as_ref() {
            if children.len() == 1 {
                self.root = Arc::clone(&children[0]);
            } else if children.is_empty() {
                self.root = Arc::new(Node::empty_leaf());
                break;
            } else {
                break;
            }
        }
    }

    /// Visits only the items whose dimension span `[item_start, item_end)` overlaps
    /// `[start_dim, end_dim)` in $O(\log N + K)$ time.
    pub fn visit_dimension_range<D, F>(&self, start_dim: usize, end_dim: usize, mut visitor: F)
    where
        D: Dimension<T::Summary>,
        F: FnMut(usize, usize, &T),
    {
        if start_dim >= end_dim || self.is_empty() {
            return;
        }
        let mut cur_idx = 0usize;
        let mut cur_dim = 0usize;
        self.root.visit_dimension_range::<D, F>(
            start_dim,
            end_dim,
            &mut cur_idx,
            &mut cur_dim,
            &mut visitor,
        );
    }

    /// Finds the last item matching `item_matcher(item_start_dim, item)` while pruning subtrees
    /// rejected by `subtree_predicate(subtree_start_dim, subtree_summary)` in $O(\log N)$ time.
    pub fn rfind<D, P, M>(
        &self,
        subtree_predicate: P,
        item_matcher: M,
    ) -> Option<(usize, usize, &T)>
    where
        D: Dimension<T::Summary>,
        P: Fn(usize, &T::Summary) -> bool,
        M: Fn(usize, &T) -> bool,
    {
        if self.is_empty() {
            return None;
        }
        self.root
            .rfind_before_or_after::<D, P, M>(0, 0, &subtree_predicate, &item_matcher)
    }

    /// Finds the first item matching `item_matcher(item_idx, item_start_dim, item)` while pruning
    /// subtrees rejected by `subtree_predicate(subtree_start_dim, subtree_summary)` in $O(\log N)$ time.
    pub fn find_first<D, P, M>(
        &self,
        subtree_predicate: P,
        item_matcher: M,
    ) -> Option<(usize, usize, &T)>
    where
        D: Dimension<T::Summary>,
        P: Fn(usize, &T::Summary) -> bool,
        M: Fn(usize, usize, &T) -> bool,
    {
        if self.is_empty() {
            return None;
        }
        self.root
            .find_first::<D, P, M>(0, 0, &subtree_predicate, &item_matcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    struct TestSum {
        lines: usize,
        anchors: usize,
    }

    impl Summary for TestSum {
        fn combine(left: &Self, right: &Self) -> Self {
            Self {
                lines: left.lines + right.lines,
                anchors: left.anchors + right.anchors,
            }
        }
    }

    #[derive(Debug, Clone)]
    struct TestCell {
        id: usize,
        lines: usize,
        is_anchor: bool,
    }

    impl Summarizable for TestCell {
        type Summary = TestSum;

        fn summary(&self) -> Self::Summary {
            TestSum {
                lines: self.lines,
                anchors: usize::from(self.is_anchor),
            }
        }
    }

    struct LineDim;
    impl Dimension<TestSum> for LineDim {
        fn measure(summary: &TestSum) -> usize {
            summary.lines
        }
    }

    #[test]
    fn test_sum_tree_push_set_truncate_and_viewport_range() {
        let mut tree = SumTree::new();
        let mut expected_total = 0usize;
        for i in 0..500 {
            let lines = (i % 7) + 1;
            let is_anchor = i % 25 == 0;
            expected_total += lines;
            tree.push(TestCell {
                id: i,
                lines,
                is_anchor,
            });
        }
        assert_eq!(tree.len(), 500);
        assert_eq!(tree.summary().lines, expected_total);
        assert_eq!(tree.summary().anchors, 20);

        // Mutate item 123 in-place (+10 lines)
        let prev = tree.get(123).expect("item 123").clone();
        assert!(tree.set(
            123,
            TestCell {
                id: 123,
                lines: prev.lines + 10,
                is_anchor: true,
            }
        ));
        assert_eq!(tree.summary().lines, expected_total + 10);
        assert_eq!(tree.summary().anchors, 21);

        // Viewport slice [500..540) visits only overlapping items
        let mut visited = Vec::new();
        tree.visit_dimension_range::<LineDim, _>(500, 540, |idx, start_line, cell| {
            assert!(start_line + cell.lines > 500);
            assert!(start_line < 540);
            visited.push((idx, start_line, cell.id));
        });
        assert!(!visited.is_empty());
        assert!(visited.len() <= 40);

        // Reverse anchor lookup before row 800
        let found = tree.rfind::<LineDim, _, _>(
            |sub_start, sum| sum.anchors > 0 && sub_start < 800,
            |item_start, cell| cell.is_anchor && item_start < 800,
        );
        let (anchor_idx, anchor_row, anchor_cell) = found.expect("anchor before row 800");
        assert!(anchor_cell.is_anchor);
        assert!(anchor_row < 800);

        // Forward lookup for next anchor after `anchor_idx`
        let next = tree.find_first::<LineDim, _, _>(
            |sub_start, sum| sum.anchors > 0 && sub_start + sum.lines > anchor_row,
            |idx, _start, cell| idx > anchor_idx && cell.is_anchor,
        );
        let (next_idx, next_row, _) = next.expect("next anchor");
        assert!(next_idx > anchor_idx);
        assert!(next_row > anchor_row);

        // Truncate down to 50 items
        tree.truncate(50);
        assert_eq!(tree.len(), 50);
        let manual_sum: usize = (0..50).map(|i| tree.get(i).expect("get").lines).sum();
        assert_eq!(tree.summary().lines, manual_sum);
    }
}
