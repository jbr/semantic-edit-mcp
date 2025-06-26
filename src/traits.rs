use crate::types::{Example, ToolSchema};
use schemars::{
    generate::SchemaSettings,
    transform::{RecursiveTransform, Transform},
    JsonSchema, Schema,
};
use serde::Serialize;
use serde_json::Value;

pub trait WithExamples: Sized + Serialize {
    fn examples() -> Option<Vec<Example<Self>>> {
        None
    }
}

fn remove_null(schema: &mut Schema) {
    if let Some(a @ Value::Array(_)) = schema.get_mut("type") {
        let arr = a.as_array_mut().unwrap();
        arr.retain(|v| matches!(v, Value::String(s) if s != "null"));
        if arr.len() == 1 {
            *a = arr.pop().unwrap();
        }
    }

    if let Some(a @ Value::Array(_)) = schema.get_mut("enum") {
        let arr = a.as_array_mut().unwrap();
        arr.retain(|v| matches!(v, Value::String(s) if s != "null"));
    }
}

pub trait AsToolSchema {
    fn as_tool_schema() -> ToolSchema;
}

impl<T> AsToolSchema for T
where
    T: JsonSchema + WithExamples,
{
    fn as_tool_schema() -> ToolSchema {
        let settings = SchemaSettings::draft2020_12().with(|s| {
            s.meta_schema = None;
            s.inline_subschemas = true;
        });

        let generator = settings.into_generator();
        let mut schema = generator.into_root_schema_for::<Self>();

        RecursiveTransform(remove_null).transform(&mut schema);

        let name = schema
            .remove("title")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let description = schema
            .remove("description")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        schema.remove("$schema");

        if let Some(examples) = Self::examples() {
            schema.insert(
                "examples".to_string(),
                serde_json::to_value(examples).unwrap(),
            );
        }

        match serde_json::from_value(schema.clone().into()) {
            Ok(input_schema) => ToolSchema {
                name,
                description: Some(description),
                input_schema,
            },
            Err(e) => {
                let json = serde_json::to_string_pretty(&schema).unwrap();
                eprintln!("{json}");
                log::error!("{json}");
                panic!("{e}")
            }
        }

        // let input_schema = serde_json::from_value(schema.into()).unwrap();

        // ToolSchema {
        //     name,
        //     description: Some(description),
        //     input_schema,
        // }
    }
}
