use crate::state::SemanticEditTools;

mcplease::tools!(
    SemanticEditTools,
    (StageOperation, stage_operation, "stage_operation"),
    (RetargetStaged, retarget_staged, "retarget_staged"),
    (CommitStaged, commit_staged, "commit_staged"),
    (SetContext, set_context, "set_context")
);
