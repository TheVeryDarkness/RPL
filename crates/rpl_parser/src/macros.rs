#[macro_export]
macro_rules! collect_elems_separated_by_comma {
    ($decls:expr) => {{
        let (first, following, _) = $decls.get_matched();
        let following = following
            .iter_matched()
            .map(|comma_with_elem| comma_with_elem.get_matched().1);
        std::iter::once(first).chain(following)
    }};
}
