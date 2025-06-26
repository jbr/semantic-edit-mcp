use crate::state::SemanticEditTools;
use crate::traits::AsToolSchema;
use crate::types::ToolSchema;
use anyhow::Result;

macro_rules! tools {
    ($state:tt, $(($capitalized:tt, $lowercase:tt, $string:literal)),+) => {
        $(mod $lowercase;)+
        $(pub use $lowercase::$capitalized;)+

        #[derive(Debug, serde::Serialize, serde::Deserialize)]
        #[serde(tag = "name")]
        pub enum Tools {
            $(#[serde(rename = $string)] $capitalized { arguments: $capitalized },)+
        }

        impl Tools {
            pub fn execute(self, state: &mut $state) -> Result<String> {
                match self {
                    $(Tools::$capitalized { arguments} => arguments.execute(state),)+
                }
            }

            pub fn schema() -> Vec<ToolSchema> {
                vec![$($capitalized::as_tool_schema(),)+]
            }

            #[allow(dead_code)]
            pub fn name(&self) -> &str {
                match self {
                    $(Tools::$capitalized { .. } => $string,)+
                }
            }
        }
    };
}

tools!(
    SemanticEditTools,
    (StageOperation, stage_operation, "stage_operation"),
    (RetargetStaged, retarget_staged, "retarget_staged"),
    (CommitStaged, commit_staged, "commit_staged"),
    (SetContext, set_context, "set_context")
);
