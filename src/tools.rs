use crate::state::SemanticEditTools;

mcplease::tools!(
    SemanticEditTools,
    (StageOperation, stage_operation, "stage_operation"),
    (RetargetStaged, retarget_staged, "retarget_staged"),
    (CommitStaged, commit_staged, "commit_staged"),
    (
        SetWorkingDirectory,
        set_working_directory,
        "set_working_directory"
    )
);
