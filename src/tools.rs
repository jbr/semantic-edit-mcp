use crate::state::SemanticEditTools;

mcplease::tools!(
    SemanticEditTools,
    (Edit, edit, "edit"),
    (FindAnchor, find_anchor, "find_anchor"),
    (RetargetEdit, retarget_edit, "retarget_edit"),
    (UndoEdit, undo_edit, "undo_edit"),
    (
        SetWorkingDirectory,
        set_working_directory,
        "set_working_directory"
    )
);
