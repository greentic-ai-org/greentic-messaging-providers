#![allow(unsafe_op_in_unsafe_fn)]

mod bindings {
    wit_bindgen::generate!({ path: "wit/secrets-probe", world: "secrets-probe", generate_all });
}

use bindings::Guest;
use bindings::greentic::secrets_store::secrets_store;

struct Component;

impl Guest for Component {
    fn run() -> String {
        match secrets_store::get("TEST_API_KEY") {
            Ok(Some(_)) => r#"{"ok":true,"key_present":true}"#.to_string(),
            Ok(None) | Err(_) => r#"{"ok":false,"key_present":false}"#.to_string(),
        }
    }
}

bindings::__export_world_secrets_probe_cabi!(Component with_types_in bindings);
