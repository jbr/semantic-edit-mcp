use crate::state::SemanticEditTools;

mcplease::tools!(
    SemanticEditTools,
    (PreviewEdit, preview_edit, "preview_edit"),
    (FindAnchor, find_anchor, "find_anchor"),
    (RetargetEdit, retarget_edit, "retarget_edit"),
    (PersistEdit, persist_edit, "persist_edit"),
    (
        SetWorkingDirectory,
        set_working_directory,
        "set_working_directory"
    )
);
