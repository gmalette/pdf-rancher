use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all(deserialize = "PascalCase"))]
pub struct License {
    component: String,
    origin: String,
    license: String,
    copyright: String,
}

static LICENSES: OnceLock<Vec<License>> = OnceLock::new();

pub fn licenses() -> Vec<License> {
    let licences = LICENSES.get_or_init(|| {
        let licenses = include_str!("../LICENSE-3rdparty.csv");
        let mut rdr = csv::Reader::from_reader(licenses.as_bytes());
        let mut records = Vec::new();
        for result in rdr.deserialize() {
            let record: License = result.unwrap();
            records.push(record);
        }
        records
    });
    licences.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_licenses() {
        let licenses = licenses();
        assert!(licenses.len() > 0);

        // tauri license
        let tauri_license = licenses.iter().find(|l| l.component == "tauri").unwrap();
        assert_eq!("https://github.com/tauri-apps/tauri", tauri_license.origin);
        assert_eq!("Apache-2.0 OR MIT", tauri_license.license);
    }
}
