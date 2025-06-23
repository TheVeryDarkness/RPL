use rustc_data_structures::fx::FxHashMap;
use rustc_span::Symbol;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ColumnType {
    Const,
    Ty,
    Place,
    Label,
}

pub(crate) type TableHead = FxHashMap<Symbol, ColumnType>;
