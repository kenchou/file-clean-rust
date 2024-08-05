use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct PatternsConfig {
    pub remove: Vec<String>,
    pub remove_hash: HashMap<String, Vec<String>>,
    pub cleanup: Vec<String>,
}

impl PatternsConfig {
    pub fn from_config_file(config_file: &Path) -> PatternsConfig {
        let file = File::open(config_file).expect("Cannot open file!");
        let values: HashMap<String, serde_yaml::Value> = serde_yaml::from_reader(file).unwrap();
        let mut config = PatternsConfig {
            remove: vec![],
            remove_hash: HashMap::new(),
            cleanup: vec![],
        };
        for (key, value) in values {
            match key.as_str() {
                "remove" => match value {
                    serde_yaml::Value::String(s) => config
                        .remove
                        .extend(s.lines().map(|v| v.trim().to_string()).collect::<Vec<_>>()),
                    serde_yaml::Value::Sequence(s) => config.remove.extend(
                        s.iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<Vec<_>>(),
                    ),
                    _ => {}
                },
                "remove_hash" => {
                    if let serde_yaml::Value::Mapping(map) = value {
                        config.remove_hash.extend(
                            map.iter()
                                .map(|(k, v)| {
                                    (
                                        k.as_str().unwrap().to_string(),
                                        match v {
                                            serde_yaml::Value::Sequence(hash_list) => hash_list
                                                .iter()
                                                .map(|vv| vv.as_str().unwrap().to_string())
                                                .collect(),
                                            _ => vec![],
                                        },
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )
                    }
                }
                "cleanup" => match value {
                    serde_yaml::Value::String(s) => config
                        .cleanup
                        .extend(s.lines().map(|v| v.trim().to_string()).collect::<Vec<_>>()),
                    serde_yaml::Value::Sequence(s) => config.cleanup.extend(
                        s.iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<Vec<_>>(),
                    ),
                    _ => {}
                },
                _ => {}
            }
        }
        config
    }
}
//EOP
