//! Exporting a collection to other tools. Each format's writer is pure:
//! it takes [`Entry`]s and returns a document. [`collection::read_entries`]
//! reads the files into entries first.

pub mod postman;

use crate::model::RequestFile;

/// A folder or request, as the exporters see a collection. Entries are built
/// once per export and dropped after it, so the size gap between the
/// variants costs nothing that matters.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Entry {
    Folder { name: String, items: Vec<Entry> },
    Request { name: String, req: RequestFile },
}
